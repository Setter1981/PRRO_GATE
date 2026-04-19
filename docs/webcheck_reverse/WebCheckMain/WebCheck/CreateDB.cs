using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class CreateDB
{
	private string PathDB;

	public CreateDB(string fnS)
	{
		PathDB = "";
		string text = All.MyDoc() + "\\WebCheck\\DB\\" + fnS + ".db";
		PathDB = "Data Source=" + text + "; Version=3";
	}

	public void CreateTriger()
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "CREATE TRIGGER checkcount AFTER INSERT ON ksef \r\n            BEGIN \r\n            UPDATE SHIFTS SET \r\n            LastLocalCheckNumber=LastLocalCheckNumber+1 WHERE \r\n            (NEW.shiftid = SHIFTS.ID AND NEW.DocType <> 8); \r\n            END";
			sQLiteCommand.ExecuteNonQuery();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void CreateTriger1()
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "CREATE TRIGGER fnsupdate10 AFTER INSERT ON ksef \r\n            BEGIN \r\n            UPDATE fns SET \r\n            used=datetime(CURRENT_TIMESTAMP, 'localtime') WHERE \r\n            (fns.checkidfiscal= NEW.checkidficscal AND NEW.offline =2 ); \r\n            END";
			sQLiteCommand.ExecuteNonQuery();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void CreateTriger2()
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS ksefup8 AFTER INSERT ON ksef  WHEN (NEW.DocType=80) \r\n            BEGIN  \r\n            Delete from CHECKPAY  WHERE checkid in (SELECT id from CHECKHEAD where (CHECKHEAD.shiftid < (NEW.shiftid-1))); \r\n\t\t\tDelete from CHECKBODY  WHERE checkid in (SELECT id from CHECKHEAD where (CHECKHEAD.shiftid < (NEW.shiftid-1))); \r\n\t\t\tDelete from CHECKTAX  WHERE checkid in (SELECT id from CHECKHEAD where (CHECKHEAD.shiftid < (NEW.shiftid-1))); \r\n            END;";
			sQLiteCommand.ExecuteNonQuery();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void CreateTriger3()
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "CREATE TRIGGER fnsupdateerror5 AFTER UPDATE ON ksef  \r\n            BEGIN  \r\n            UPDATE fns SET  \r\n            used=datetime(CURRENT_TIMESTAMP, 'localtime') WHERE  \r\n            (fns.checkidfiscal= NEW.checkidficscal AND NEW.offline =3 );  \r\n            END";
			sQLiteCommand.ExecuteNonQuery();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void CreateTrigerBackup()
	{
		int num = 0;
		do
		{
			CreateTrigerBackupAll(num);
			num = checked(num + 1);
		}
		while (num <= 18);
	}

	private void CreateTrigerBackupAll(int nt)
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			switch (nt)
			{
			case 0:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS OPERATORS1 AFTER UPDATE ON OPERATORS \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('UPDATE OPERATORS SET OPERATORNAME='''||NEW.OPERATORNAME||''',KEYPATH='''||NEW.KEYPATH||''',KEYPASS='''||NEW.KEYPASS||''',INN='''||NEW.INN||''' WHERE id = '''||NEW.ID||''''); \r\n            END;";
				break;
			case 1:
				sQLiteCommand.CommandText = "CREATE TRIGGER  IF NOT EXISTS  OPERATORS2 AFTER INSERT ON OPERATORS \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace INTO OPERATORS (id,OPERATORNAME,KEYPATH,KEYPASS,INN) VALUES ('''||NEW.ID||''','''||NEW.OPERATORNAME||''','''||NEW.KEYPATH||''','''||NEW.KEYPASS||''','''||NEW.INN||''')'); \r\n            END;";
				break;
			case 2:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS SHIFTS2 AFTER UPDATE OF DATEEND ON SHIFTS  \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('UPDATE SHIFTS SET DATEEND='''||NEW.DATEEND||''' WHERE ID = '''||NEW.ID||''''); \r\n            END;";
				break;
			case 3:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS SHIFTS3 AFTER UPDATE OF localchecknumber ON SHIFTS  \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('UPDATE SHIFTS SET localchecknumber='''||NEW.localchecknumber||''' WHERE id = '''||NEW.ID||''''); \r\n            END;";
				break;
			case 4:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS ksefup1 AFTER INSERT ON ksef WHEN ( NEW.offline=3 or NEW.offline=0 or NEW.offline=1) BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert into ksef (checkidficscal,localchecknumber,DocType,sum,shiftid,dt,id,offline,checkid,checkxml) values('''||NEW.checkidficscal||''','''||NEW.localchecknumber||''','''||NEW.DocType||''','''||NEW.sum||''','''||NEW.shiftid||''','''||NEW.dt||''','''||NEW.id||''','''||NEW.offline||''','''||NEW.checkid||''','''||NEW.checkxml||''')'); END;";
				break;
			case 5:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS ksefup2 AFTER UPDATE OF offline ON ksef  WHEN (NEW.offline <> 1) BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('UPDATE ksef SET offline='''||NEW.offline||''' WHERE id = '''||NEW.ID||'''');  END;";
				break;
			case 6:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS ksefup3 AFTER INSERT ON ksef WHEN (NEW.offline=2) BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace into ksef (checkidficscal,localchecknumber,DocType,sum,shiftid,dt,id,offline,checkxml,checksigned,checkid) values('''||NEW.checkidficscal||''','''||NEW.localchecknumber||''','''||NEW.DocType||''','''||NEW.sum||''','''||NEW.shiftid||''','''||NEW.dt||''','''||NEW.id||''','''||NEW.offline||''','''||NEW.checkxml||''','''||NEW.checksigned||''','''||NEW.checkid||''')'); END;";
				break;
			case 7:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS ksefup4 AFTER UPDATE OF offline ON ksef  WHEN (NEW.offline = 1 ) \r\n            BEGIN INSERT INTO  backuplog(tobackup) Values ('UPDATE ksef SET offline='''||NEW.offline||''',checksigned='''' WHERE id='''||NEW.ID||''''); END;";
				break;
			case 8:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS payforms1 AFTER INSERT ON PayForms \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace INTO PayForms (id,name,ISCASH) VALUES ('''||NEW.ID||''','''||NEW.name||''','''||NEW.ISCASH||''')'); \r\n            END;";
				break;
			case 9:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS payforms2 AFTER UPDATE ON PayForms \r\n                    BEGIN  \r\n                    INSERT INTO  backuplog(tobackup) Values ('UPDATE PayForms SET name='''||NEW.name||''',iscash='''||NEW.iscash||''' where id='''||NEW.id||''' '); \r\n                    END;";
				break;
			case 10:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS shifts1 AFTER INSERT ON SHIFTS  \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace into SHIFTS (id,DATEBEG,DATEEND,ONAME,TAXTIN,TAXNAME,RROFISCAL,RROLOCAL,OPERATORID,LastLocalCheckNumber) values('''||NEW.id||''','''||NEW.DATEBEG||''',''NULL'','''||NEW.ONAME||''','''||NEW.TAXTIN||''','''||NEW.TAXNAME||''','''||NEW.RROFISCAL||''','''||NEW.RROLOCAL||''','''||NEW.OPERATORID||''',''0'') '); \r\n            END;";
				break;
			case 11:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS taxob AFTER INSERT ON TAXOBJECTS  \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace into TAXOBJECTS (id,FN,TIN,INN,POINTNAME,ORGNAME,POINTADDR) values('''||NEW.id||''','''||NEW.FN||''','''||NEW.TIN||''','''||NEW.INN||''','''||NEW.POINTNAME||''','''||NEW.ORGNAME||''','''||NEW.POINTADDR||''')'); \r\n            END;";
				break;
			case 12:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS taxobj2 AFTER UPDATE ON TAXOBJECTS \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('UPDATE TAXOBJECTS SET FN='''||NEW.FN||''',TIN='''||NEW.TIN||''',INN='''||NEW.INN||''',POINTNAME='''||NEW.POINTNAME||''',ORGNAME='''||NEW.ORGNAME||''',POINTADDR='''||NEW.POINTADDR||''' WHERE id = '''||NEW.ID||''''); \r\n            END;";
				break;
			case 13:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS checkbody1 AFTER INSERT ON CHECKBODY  \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace into CHECKBODY (id,checkid,code,LETTER,COST) values('''||NEW.id||''','''||NEW.checkid||''','''||NEW.code||''','''||NEW.LETTER||''','''||NEW.COST||''')'); \r\n            END;";
				break;
			case 14:
				sQLiteCommand.CommandText = "CREATE TRIGGER  IF NOT EXISTS  checkhead1 AFTER INSERT ON checkhead  \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace into checkhead (id,shiftid,doctype,ordernum,totalsum) values('''||NEW.id||''','''||NEW.shiftid||''','''||NEW.doctype||''','''||NEW.ordernum||''','''||NEW.totalsum||''')'); \r\n            END";
				break;
			case 15:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS checkpay1 AFTER INSERT ON CHECKPAY \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace into CHECKPAY (id,checkid,PAYMENTFORM,TOTALSUM) values('''||NEW.id||''','''||NEW.checkid||''','''||NEW.PAYMENTFORM||''','''||NEW.TOTALSUM||''')'); \r\n            END;";
				break;
			case 16:
				sQLiteCommand.CommandText = "CREATE TRIGGER IF NOT EXISTS  checktax1 AFTER INSERT ON CHECKtax \r\n            BEGIN  \r\n            INSERT INTO  backuplog(tobackup) Values ('insert or replace into CHECKtax (id,checkid,TAXCODE,TAXPRC,TAXSUM) values('''||NEW.id||''','''||NEW.checkid||''','''||NEW.TAXCODE||''','''||NEW.TAXCODE||''','''||NEW.TAXPRC||''')'); \r\n            END";
				break;
			case 17:
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				return;
			case 18:
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				return;
			}
			sQLiteCommand.ExecuteNonQuery();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void CreateTable(int nt)
	{
		if (nt > 13)
		{
			return;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			switch (nt)
			{
			case 0:
				sQLiteCommand.CommandText = "CREATE TABLE ksef(\r\n                    checkid TEXT,\r\n                    checkxml TEXT,\r\n                    checksigned TEXT,\r\n                    signedanswerfromficscal TEXT,\r\n                    checkidficscal TEXT,\r\n                    localchecknumber Integer,\r\n                    DocType Integer,\r\n                    sum DECIMAL(17 , 2),\r\n                    mac TEXT,\r\n                    shiftid INTEGER,\r\n                    dt DATETIME,\r\n                    ID Integer PRIMARY KEY AUTOINCREMENT,\r\n                    offline INTEGER DEFAULT 0);";
				break;
			case 1:
				sQLiteCommand.CommandText = "CREATE TABLE SHIFTS(\r\n\t                ID INTEGER PRIMARY KEY AUTOINCREMENT,\r\n\t                SHIFTID\tINTEGER,\r\n\t                DATEBEG\tDATETIME,\r\n\t                DATEEND\tDATETIME,\r\n\t                ODRFO VARCHAR(12),\r\n\t                ONAME VARCHAR(200),\r\n\t                TAXTIN VARCHAR(12),\r\n\t                TAXNAME\tVARCHAR(300),\r\n\t                RROFISCAL BIGINT,\r\n\t                RROLOCAL BIGINT,\r\n                    OPERATORID INTEGER,\r\n                    LASTFISCALCHECKNUMBER INTEGER,\r\n                    LastLocalCheckNumber INTEGER);";
				break;
			case 2:
				sQLiteCommand.CommandText = "CREATE TABLE OPERATORS (\r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT,\r\n                    OPERATORNAME VARCHAR(256),\r\n                    KEYPATH VARCHAR(256),\r\n                    KEYPASS VARCHAR(256),\r\n                    INN VARCHAR(256));";
				break;
			case 3:
				sQLiteCommand.CommandText = "CREATE TABLE PayForms (\r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT,\r\n                    NAME VARCHAR(256),\r\n                    ISCASH INTEGER);";
				break;
			case 4:
				sQLiteCommand.CommandText = "CREATE TABLE TAXES (\r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT,\r\n                    NAME TEXT,\r\n                    EXCISE INTEGER,\r\n                    TAXPRC DECIMAL(17,2));";
				break;
			case 5:
				sQLiteCommand.CommandText = "CREATE TABLE TAXOBJECTS (\r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT,\r\n                    FN INTEGER,\r\n                    TIN VARCHAR(16),\r\n                    INN INTEGER,\r\n                    POINTNAME VARCHAR(256),\r\n                    ORGNAME VARCHAR(256),\r\n                    POINTADDR VARCHAR(256));";
				break;
			case 6:
				sQLiteCommand.CommandText = "CREATE TABLE CHECKHEAD ( \r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT, \r\n                    SHIFTID INTEGER, \r\n                    UID VARCHAR(36), \r\n                    DOCTYPE INT, \r\n                    VER INT, \r\n                    TIN VARCHAR(10), \r\n                    INN VARCHAR(12), \r\n                    ORGNAME VARCHAR(256), \r\n                    POINTNAME VARCHAR(256), \r\n                    POINTADDR VARCHAR(256), \r\n                    ORDERDATE DATETIME, \r\n                    ORDERNUM TEXT, \r\n                    ORDERTAXNUM TEXT, \r\n                    CASHDESKNUM BIGINT, \r\n                    FN BIGINT, \r\n                    CASHIER VARCHAR(128), \r\n                    TOTALSUM DECIMAL(17 , 2));";
				break;
			case 7:
				sQLiteCommand.CommandText = "CREATE TABLE CHECKPAY ( \r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT, \r\n                    CHECKID INTEGER, \r\n                    PAYMENTFORM VARCHAR(64), \r\n                    TOTALSUM DECIMAL(17 , 2));";
				break;
			case 8:
				sQLiteCommand.CommandText = "CREATE TABLE CHECKTAX ( \r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT, \r\n                    CHECKID INTEGER, \r\n                    TAXCODE VARCHAR(3), \r\n                    TAXPRC DECIMAL(17 , 2), \r\n                    TAXSUM DECIMAL(17 , 2));";
				break;
			case 9:
				sQLiteCommand.CommandText = "CREATE TABLE CHECKBODY ( \r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT, \r\n                    CHECKID INTEGER, \r\n                    CODE VARCHAR(64), \r\n                    UKTZED VARCHAR(15), \r\n                    GOODSNAME VARCHAR(128), \r\n                    UNITCODE VARCHAR(128), \r\n                    UNITNAME VARCHAR(64), \r\n                    AMOUNT DECIMAL(17 , 3), \r\n                    PRICE DECIMAL(17 , 2), \r\n                    LETTER VARCHAR(1), \r\n                    COST DECIMAL(17 , 2), \r\n                    LETTEREXCISE VARCHAR(1));";
				break;
			case 10:
				sQLiteCommand.CommandText = "CREATE TABLE CHECKEXCISE ( \r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT, \r\n                    CHECKID INTEGER, \r\n                    EXCISECODE VARCHAR(64), \r\n                    EXCISEPRC DECIMAL(17 , 2), \r\n                    EXCISESUM DECIMAL(17 , 2));";
				break;
			case 11:
				sQLiteCommand.CommandText = "CREATE TABLE fns (\r\n                    id INTEGER PRIMARY KEY AUTOINCREMENT UNIQUE,\r\n                    checkidfiscal TEXT,\r\n                    used DATETIME,\r\n                    added DATETIME DEFAULT CURRENT_TIMESTAMP,\r\n                    sourceid TEXT);";
				break;
			case 12:
				sQLiteCommand.CommandText = "CREATE TABLE Sessions (\r\n                    id INTEGER PRIMARY KEY AUTOINCREMENT UNIQUE,\r\n                    SessionStartDT DATETIME,\r\n                    SessionStatus INTEGER);";
				break;
			case 13:
				sQLiteCommand.CommandText = "CREATE TABLE IF NOT EXISTS backuplog ( \r\n                    id INTEGER PRIMARY KEY AUTOINCREMENT, \r\n                    tobackup TEXT, \r\n                    cmt INTEGER, \r\n                    added DATETIME DEFAULT CURRENT_TIMESTAMP);";
				break;
			}
			sQLiteCommand.ExecuteNonQuery();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void SaveTaxObjects(string fnS, string tinS, string innS, string pointName, string orgName, string pointAddr)
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO TAXOBJECTS VALUES ('1','" + fnS + "','" + tinS + "','" + innS + "','" + pointName + "','" + orgName + "','" + pointAddr + "')";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void SaveOperators(string FioS, string PathKS, string PassS, string InnS)
	{
		try
		{
			PassS = new Coding().Cod(PassS.Trim());
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO OPERATORS VALUES (1, '" + FioS + "', '" + PathKS + "', '" + PassS + "', '" + InnS + "' )";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public void SaveTaxes(string NameS, string ExS, string TaxPrS)
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO TAXES (NAME, EXCISE, TAXPRC) VALUES ('" + NameS + "'," + ExS + "," + TaxPrS + ")";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	public TypErr SavePayForms(string NameS, string IsCashS)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		TypErrStr typErrStr = All.l.MaxID("PayForms");
		if (Versioned.IsNumeric((object)typErrStr.ReturnStr) && checked(Conversions.ToInteger(typErrStr.ReturnStr) + 1) > 180)
		{
			result.errCode = 51;
			result.errStr = "Число платежей превысило лимит";
			return result;
		}
		if (SearchPayForms(NameS))
		{
			result.errCode = 51;
			result.errStr = "Такое название платежа уже есть";
			return result;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO PayForms (NAME, ISCASH) VALUES ('" + NameS + "','" + IsCashS + "')";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 51;
			result.errStr = "Ошибка добавления платежа в таблицу PayForms: " + ex2.Message;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErr UpdatePayForms(string eNamePay, string eNameOldPay, string eIsCashPay, string eidPay)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		if (Operators.CompareString(eNameOldPay, eNamePay, false) != 0 && SearchPayForms(eNamePay))
		{
			result.errCode = 51;
			result.errStr = "Такое название платежа уже есть";
			return result;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE PayForms SET name='" + eNamePay + "', iscash='" + eIsCashPay + "' where id='" + eidPay + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка при попытке изменить запись в таблице PayForms: " + ex2.Message;
			result.errCode = 106;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal bool SearchPayForms(string NameS)
	{
		if (All.PayTax.NamePayToIndex(NameS) > 0)
		{
			return true;
		}
		return false;
	}

	public void SaveInfoTable(int nnn)
	{
		if (nnn < 1 || nnn > 14)
		{
			return;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = PathDB;
			sQLiteConnection.Open();
			SQLiteDataReader sQLiteDataReader;
			switch (nnn)
			{
			case 1:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (1,'А',0,20)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 2:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (2,'Б',0,0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 3:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (3,'В',0,7)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 4:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO PayForms VALUES (1,'Готівка',1)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 5:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO PayForms VALUES (2,'Картка',2)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 6:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO PayForms VALUES (3,'Кредит',3)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 7:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO PayForms VALUES (4,'Сертифікат',4)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 8:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (4,'ГА',5,20)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 9:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (5,'ГБ',5,0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 10:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (6,'ДА',7.5,20)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 11:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (7,'ДБ',7.5,0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 12:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (8,'Е',0,0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 13:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (9,'Ж',0,0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 14:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXES VALUES (10,'З',0,0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			default:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	internal bool TableTrue(string TableName, bool newPRO)
	{
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			if (newPRO)
			{
				sQLiteConnection.ConnectionString = PathDB;
			}
			else
			{
				sQLiteConnection.ConnectionString = All.A.Connection;
			}
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT name FROM sqlite_master WHERE type='table' AND name='" + TableName + "';";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			result = (sQLiteDataReader.Read() ? true : false);
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal bool CreateSessions(bool newPRO)
	{
		bool result;
		if (TableTrue("Sessions", newPRO))
		{
			result = true;
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				if (newPRO)
				{
					sQLiteConnection.ConnectionString = PathDB;
				}
				else
				{
					sQLiteConnection.ConnectionString = All.A.Connection;
				}
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "CREATE TABLE Sessions (\r\n                    id INTEGER PRIMARY KEY AUTOINCREMENT UNIQUE,\r\n                    SessionStartDT DATETIME,\r\n                    SessionStatus INTEGER);";
				sQLiteCommand.ExecuteNonQuery();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
				goto IL_007d;
			}
			result = true;
		}
		goto IL_007d;
		IL_007d:
		return result;
	}

	internal bool CreateIndex(bool newPRO)
	{
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			if (newPRO)
			{
				sQLiteConnection.ConnectionString = PathDB;
			}
			else
			{
				sQLiteConnection.ConnectionString = All.A.Connection;
			}
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "CREATE INDEX IF NOT EXISTS  offlineind ON ksef (offline); \r\nCREATE UNIQUE INDEX IF NOT EXISTS 'closeshiftind' ON 'ksef' ('shiftid','DocType') WHERE doctype = '80' and offline  <> '-1';\r\nCREATE UNIQUE INDEX IF NOT EXISTS 'openshiftind' ON 'ksef' ('shiftid','DocType') WHERE doctype = '8' and offline  <> '-1';\r\nCREATE INDEX IF NOT EXISTS checkidind ON ksef (checkid);  \r\nCREATE INDEX IF NOT EXISTS shiftidind ON ksef (shiftid); \r\nCREATE INDEX IF NOT EXISTS DocTypeind ON ksef (DocType); \r\nCREATE UNIQUE INDEX IF NOT EXISTS shiftuniq ON shifts (DATEEND); \r\nCREATE UNIQUE INDEX IF NOT EXISTS checkiddt1 ON ksef (checkid) WHERE doctype = '80'; \r\nCREATE INDEX IF NOT EXISTS checkidficscalind ON ksef (checkidficscal);";
			sQLiteCommand.ExecuteNonQuery();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_006a;
		}
		result = true;
		goto IL_006a;
		IL_006a:
		return result;
	}
}
