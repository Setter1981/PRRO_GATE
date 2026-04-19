using System;
using System.ComponentModel;
using System.Data.SQLite;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class SQLlite
{
	public struct TypErrTaxObj
	{
		public int errCode;

		public string errStr;

		public string tTIN;

		public string tINN;

		public string tPointName;

		public string tOrgName;

		public string tPointAddr;
	}

	private string connection;

	public string Test
	{
		get
		{
			return "";
		}
		set
		{
		}
	}

	public void ConnectDB()
	{
		connection = "Data Source=" + WebCheck.All.FileN + "; Version=3";
		WebCheck.All.status = true;
	}

	public string RecordCounter(string tab)
	{
		SQLiteConnection sQLiteConnection = new SQLiteConnection();
		new SQLiteCommand();
		sQLiteConnection.ConnectionString = connection;
		sQLiteConnection.Open();
		SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
		sQLiteCommand.CommandText = "Select * FROM " + tab;
		SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
		sQLiteDataReader.Read();
		string result = sQLiteDataReader[3].ToString();
		((Component)(object)sQLiteCommand).Dispose();
		((Component)(object)sQLiteCommand).Dispose();
		sQLiteConnection.Close();
		return result;
	}

	public void CreateTable(int nt)
	{
		if (nt > 10)
		{
			return;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			switch (nt)
			{
			case 0:
				sQLiteCommand.CommandText = "CREATE TABLE test(id Integer PRIMARY KEY AUTOINCREMENT, code TEXT,  name TEXT, quantity TEXT, price TEXT, sum TEXT );";
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
				sQLiteCommand.CommandText = "CREATE TABLE TAXOBJECTS (\r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT,\r\n                    FN INTEGER,\r\n                    TIN INTEGER,\r\n                    INN INTEGER,\r\n                    POINTNAME VARCHAR(256),\r\n                    ORGNAME VARCHAR(256),\r\n                    POINTADDR VARCHAR(256));";
				break;
			case 6:
				sQLiteCommand.CommandText = "CREATE TABLE CHECKHEAD ( \r\n                    ID INTEGER PRIMARY KEY AUTOINCREMENT, \r\n                    SHIFTID INTEGER, \r\n                    UID VARCHAR(36), \r\n                    DOCTYPE INT, \r\n                    VER INT, \r\n                    TIN VARCHAR(10), \r\n                    INN VARCHAR(12), \r\n                    ORGNAME VARCHAR(256), \r\n                    POINTNAME VARCHAR(256), \r\n                    POINTADDR VARCHAR(256), \r\n                    ORDERDATE DATETIME, \r\n                    ORDERNUM BIGINT, \r\n                    ORDERTAXNUM BIGINT, \r\n                    CASHDESKNUM BIGINT, \r\n                    FN BIGINT, \r\n                    CASHIER VARCHAR(128), \r\n                    TOTALSUM DECIMAL(17 , 2));";
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

	public void CreateTriger()
	{
		SQLiteConnection sQLiteConnection = new SQLiteConnection();
		new SQLiteCommand();
		sQLiteConnection.ConnectionString = connection;
		sQLiteConnection.Open();
		SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
		sQLiteCommand.CommandText = "CREATE TRIGGER checkcount AFTER INSERT ON CHECKHEAD \r\n        BEGIN \r\n        UPDATE SHIFTS SET \r\n        LastLocalCheckNumber=LastLocalCheckNumber+1 WHERE \r\n        (NEW.SHIFTID = SHIFTS.ID  AND NEW.FN = SHIFTS.RROFISCAL); \r\n        END";
		sQLiteCommand.ExecuteNonQuery();
		((Component)(object)sQLiteCommand).Dispose();
		sQLiteConnection.Close();
	}

	public void LoatInfoTable(int nnn)
	{
		if (nnn > 7)
		{
			return;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = connection;
			sQLiteConnection.Open();
			SQLiteDataReader sQLiteDataReader;
			switch (nnn)
			{
			case 0:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO TAXOBJECTS VALUES (1,2222222222,22222222,333333333,'Имя торговой точки','ТОВ Д.Т.ІКС. СЕРВІС','Адрес торговой точки')";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
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
				sQLiteCommand.CommandText = "INSERT INTO PayForms VALUES (1,'Готівка',0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 5:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO PayForms VALUES (2,'Картка',0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 6:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO PayForms VALUES (3,'Кредит',0)";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			case 7:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO OPERATORS VALUES (1,'Чеков П.С.','путькключу','паролькключу','1111111111')";
				sQLiteDataReader = sQLiteCommand.ExecuteReader();
				break;
			default:
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO test VALUES (1,'test','test','test','test','test')";
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

	public void LoadPay(string code, string name, string quantity, string price, string sum)
	{
		SQLiteConnection sQLiteConnection = new SQLiteConnection();
		SQLiteCommand sQLiteCommand = new SQLiteCommand();
		sQLiteConnection.ConnectionString = connection;
		sQLiteConnection.Open();
		sQLiteCommand = sQLiteConnection.CreateCommand();
		sQLiteCommand.CommandText = "INSERT INTO test (code, name, quantity, price, sum ) VALUES ('" + code + "','" + name + "','" + quantity + "','" + price + "','" + sum + "')";
		SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
		((Component)(object)sQLiteCommand).Dispose();
		sQLiteDataReader.Close();
		sQLiteConnection.Close();
	}

	public string MaxID(string TAB)
	{
		string text = "0";
		string connectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = connectionString;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT MAX(ID) FROM " + TAB;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			text = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(text, "", false) == 0)
		{
			text = "0";
		}
		return text;
	}

	public TypErrTaxObj InfaTaxObjects()
	{
		TypErrTaxObj result = default(TypErrTaxObj);
		result.errCode = 0;
		result.errStr = "";
		result.tINN = "";
		result.tOrgName = "";
		result.tTIN = "";
		result.tPointName = "";
		result.tPointAddr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = "Data Source=" + WebCheck.All.FileN + "; Version=3";
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM TAXOBJECTS WHERE FN=" + WebCheck.All.FN;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.tTIN = sQLiteDataReader[2].ToString();
			result.tINN = sQLiteDataReader[3].ToString();
			result.tPointName = sQLiteDataReader[4].ToString();
			result.tOrgName = sQLiteDataReader[5].ToString();
			result.tPointAddr = sQLiteDataReader[6].ToString();
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
		return result;
	}
}
