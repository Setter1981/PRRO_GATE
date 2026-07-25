using System;
using System.ComponentModel;
using System.Data.SQLite;
using System.IO;
using System.Threading;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class SQLlite
{
	internal bool OpenShiftReturn;

	private long NumberBlock;

	internal bool BlockLoсal;

	public SQLlite()
	{
		OpenShiftReturn = true;
		NumberBlock = 0L;
		BlockLoсal = false;
	}

	internal string TextToTextSQL(string txt)
	{
		return Strings.Replace(Strings.Replace(txt, "'", "\\`"), "\"", "\\\"");
	}

	internal string TextSQLToText(string txt)
	{
		return Strings.Replace(Strings.Replace(txt, "\\`", "'"), "\\\"", "\"");
	}

	internal string TextToTextXML(string txt)
	{
		return Strings.Replace(Strings.Replace(txt, "'", ""), "\"", "");
	}

	public TypErr SaveOpenShift(string OpName, string TIN, string TAXNAME, string OPERATORID)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		if (Conversions.ToInteger(ReturnOpenShift().ReturnStr) > 0)
		{
			result.errCode = 7;
			result.errStr = "Вже є відкрита зміна";
		}
		int num = Conversions.ToInteger(MaxID("SHIFTS").ReturnStr);
		int num2 = 0;
		checked
		{
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = All.A.Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO SHIFTS \r\n(DATEBEG, DATEEND, OPERATORID, ONAME, TAXTIN, TAXNAME, RROFISCAL, RROLOCAL, LastLocalCheckNumber)\r\nVALUES \r\n(datetime(CURRENT_TIMESTAMP, 'localtime'),'NULL','" + OPERATORID + "','" + OpName + "','" + TIN + "','" + TextToTextSQL(TAXNAME) + "','" + All.A.FN + "', '1', '0')";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("SHIFTS").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 7;
					result.errStr = "Ошибка при записи в таблицу SHIFTS.";
					ProjectData.ClearProjectError();
					goto IL_015a;
				}
				break;
				IL_015a:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
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
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM TAXOBJECTS WHERE FN=" + All.A.FN;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.tTIN = sQLiteDataReader[2].ToString();
			result.tINN = sQLiteDataReader[3].ToString();
			result.tPointName = TextSQLToText(sQLiteDataReader[4].ToString());
			result.tOrgName = TextSQLToText(sQLiteDataReader[5].ToString());
			result.tPointAddr = TextSQLToText(sQLiteDataReader[6].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Нет информации в таблице TAXOBJECTS по данному FN.";
			result.errCode = 6;
			ProjectData.ClearProjectError();
		}
		if (result.tINN.Length == 10)
		{
			result.tINN = "00" + result.tINN;
		}
		return result;
	}

	public TypErrStr OperatorName(string INNo)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM OPERATORS WHERE INN=" + INNo;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[1].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 5;
			result.errStr = "Немає такого оператора.";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrStr INNoperatorInShift(string idShiftS)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM SHIFTS WHERE ID='" + idShiftS + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[10].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 0;
			result.errStr = "";
			result.ReturnStr = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrOperKeyPass OperatorInfa(string INNop)
	{
		TypErrOperKeyPass result = default(TypErrOperKeyPass);
		result.errCode = 0;
		result.errStr = "";
		result.KeyFile = "";
		result.Pass = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM OPERATORS WHERE INN=" + INNop;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.KeyFile = sQLiteDataReader[2].ToString();
			Coding coding = new Coding();
			result.Pass = coding.DeCod(sQLiteDataReader[3].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 5;
			result.errStr = "Немає такого оператора.";
			result.KeyFile = "";
			result.Pass = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrOperKeyPassNameINN OperatorInfa()
	{
		TypErrOperKeyPassNameINN result = default(TypErrOperKeyPassNameINN);
		result.errCode = 0;
		result.errStr = "";
		result.KeyFile = "";
		result.Pass = "";
		result.Name = "";
		result.INN = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM OPERATORS WHERE ID=1";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.Name = sQLiteDataReader[1].ToString();
			result.KeyFile = sQLiteDataReader[2].ToString();
			Coding coding = new Coding();
			result.Pass = coding.DeCod(sQLiteDataReader[3].ToString());
			result.INN = sQLiteDataReader[4].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 5;
			result.errStr = "Немає такого оператора.";
			result.KeyFile = "";
			result.Pass = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrOperKeyPass OperatorKeyPass(string idShiftS)
	{
		TypErrOperKeyPass result = default(TypErrOperKeyPass);
		result.errCode = 0;
		result.errStr = "";
		result.KeyFile = "";
		result.Pass = "";
		TypErrStr typErrStr = INNoperatorInShift(idShiftS);
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		result = OperatorInfa(typErrStr.ReturnStr);
		if (result.errCode > 0)
		{
			result.KeyFile = "";
			result.Pass = "";
		}
		return result;
	}

	public TypErrStr ReturnOpenShiftTime()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT DATEBEG FROM SHIFTS WHERE DATEEND = 'NULL'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnStr = sQLiteDataReader[0].ToString();
			}
			else
			{
				result.ReturnStr = "";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrStr ReturnOpenShift()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT ID FROM SHIFTS WHERE DATEEND = 'NULL'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", TextCompare: false) == 0)
		{
			result.ReturnStr = "0";
		}
		if (Operators.CompareString(result.ReturnStr, "0", TextCompare: false) != 0 && Operators.CompareString(result.ReturnStr, MaxID("SHIFTS").ReturnStr, TextCompare: false) != 0)
		{
			result.ReturnStr = "-1";
			result.errCode = 1003;
			result.errStr = "ВНИМАНИЕ! Открыто несколько смен!!!";
		}
		if (!OpenShiftReturn & (Operators.CompareString(result.ReturnStr, "0", TextCompare: false) == 0))
		{
			result.ReturnStr = MaxID("SHIFTS").ReturnStr;
		}
		return result;
	}

	public TypErrStr ReturnOpenShiftEX(string FNs)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		string text = All.f.StringGetFn(FNs, "Path");
		text = "Data Source=" + text.Trim() + "; Version=3";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = text;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT ID FROM SHIFTS WHERE DATEEND = 'NULL'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", TextCompare: false) == 0)
		{
			result.ReturnStr = "0";
		}
		if (Operators.CompareString(result.ReturnStr, "0", TextCompare: false) != 0)
		{
			string returnStr = MaxIDEX("SHIFTS", text).ReturnStr;
			if (Operators.CompareString(result.ReturnStr, returnStr, TextCompare: false) != 0)
			{
				result.ReturnStr = "-1";
				result.errCode = 1003;
				result.errStr = "ВНИМАНИЕ! Открыто несколько смен!!!";
			}
		}
		return result;
	}

	public TypErrStr MaxIDEX(string TAB, string conEX)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = conEX;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT MAX(ID) FROM " + TAB;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", TextCompare: false) == 0)
		{
			result.ReturnStr = "0";
		}
		return result;
	}

	public TypErrLLCNshift ReturnLocalCheckNumberShift()
	{
		TypErrLLCNshift result = default(TypErrLLCNshift);
		result.errCode = 0;
		result.errStr = "";
		result.LastLocalCheckNumbern = "0";
		result.IDshift = "0";
		result.Cashier = "";
		result.OperatorID = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT ID, LastLocalCheckNumber, ONAME, OPERATORID FROM SHIFTS WHERE DATEEND = 'NULL'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.IDshift = sQLiteDataReader[0].ToString();
			result.LastLocalCheckNumbern = sQLiteDataReader[1].ToString();
			result.Cashier = sQLiteDataReader[2].ToString();
			result.OperatorID = sQLiteDataReader[3].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.IDshift = "-1";
			result.LastLocalCheckNumbern = "-1";
			result.errStr = "Не могу получить текущий локальный номер чека из таблицы SHIFTS";
			result.errCode = 15;
			ProjectData.ClearProjectError();
		}
		if (!Versioned.IsNumeric(result.LastLocalCheckNumbern))
		{
			result.IDshift = "-1";
			result.LastLocalCheckNumbern = "-1";
			result.errStr = "Локальный номер чека не является числом - талица SHIFTS";
			result.errCode = 15;
		}
		return result;
	}

	public TypErrStr MaxID(string TAB)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT MAX(ID) FROM " + TAB;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", TextCompare: false) == 0)
		{
			result.ReturnStr = "0";
		}
		return result;
	}

	public TypErrStr CountMax(string TAB)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT count(*) FROM " + TAB;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", TextCompare: false) == 0)
		{
			result.ReturnStr = "0";
		}
		return result;
	}

	public TypErrStr TestBug(int e)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			switch (e)
			{
			case 1:
				sQLiteCommand.CommandText = "Select id from ksef where (DocType = 8 and (signedanswerfromficscal = '' or signedanswerfromficscal = 'not') and offline = 1 and  id > (select id FROM ksef where (DocType = 9 and (offline = 2 or offline =3)))) Limit 1";
				break;
			case 2:
				sQLiteCommand.CommandText = "Select DT from ksef where (shiftid = (Select id from shifts where DATEEND = 'NULL' LIMIT 1)and DocType = 80)";
				break;
			case 3:
				sQLiteCommand.CommandText = "Select (case when count(shiftid)<2 then 0 else shiftid end) as shiftidsq from ksef where (DocType = 8 and offline <> -1 ) group by shiftid  order by shiftidsq DESC LIMIT 1";
				break;
			case 4:
				sQLiteCommand.CommandText = "Select (case when count(shiftid)<2 then 0 else shiftid end) as shiftidsq from ksef where (DocType = 80 and offline <> -1 ) group by shiftid  order by shiftidsq DESC LIMIT 1";
				break;
			case 5:
				sQLiteCommand.CommandText = "Select id from ksef where (offline=1 and (signedanswerfromficscal = '' or signedanswerfromficscal = 'not') and id= (select id from ksef where (offline=3) LIMIT 1 )+1  ) LIMIT 1";
				break;
			default:
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				result.ReturnStr = "";
				return result;
			}
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			result.ReturnStr = result.ReturnStr.Trim();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErr BugFix(int e)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			switch (e)
			{
			case 1:
				sQLiteCommand.CommandText = "DELETE FROM 'shifts' WHERE id = (SELECT id FROM 'shifts' WHERE DATEEND = 'NULL' ORDER BY id  DESC LIMIT 1)";
				break;
			case 2:
				sQLiteCommand.CommandText = "UPDATE sqlite_sequence SET seq = (SELECT id FROM 'shifts' ORDER BY id  DESC LIMIT 1) WHERE  name='SHIFTS'";
				break;
			case 3:
				sQLiteCommand.CommandText = "Update ksef Set  offline = 2 where id  = (select id from ksef where (DocType = 8 and (signedanswerfromficscal = '' or signedanswerfromficscal = 'not') and offline = 1 and  id > (select id FROM ksef where (DocType = 9 and (offline = 2 or offline =3)))) Limit 1)";
				break;
			case 4:
				sQLiteCommand.CommandText = "Update shifts set DATEEND = ( Select DT from ksef where (shiftid = (Select id from shifts where DATEEND = 'NULL' LIMIT 1)and DocType = 80) limit 1) where DATEEND = 'NULL'";
				break;
			case 5:
				sQLiteCommand.CommandText = "Update ksef set offline = -1 where id=( select id from ksef where shiftid = (select (case when count(shiftid)<2 then 0 else shiftid end) as shiftidsq from ksef where (DocType = 8 and offline <> -1 ) group by shiftid  order by shiftidsq DESC  LIMIT 1 ) and DocType =8 order by id desc limit 1 )";
				break;
			case 6:
				sQLiteCommand.CommandText = "Update ksef set offline = -1 where id=( select id from ksef where shiftid = (select (case when count(shiftid)<2 then 0 else shiftid end) as shiftidsq from ksef where (DocType = 80 and offline <> -1 ) group by shiftid  order by shiftidsq DESC  LIMIT 1 ) and DocType =80 order by id desc limit 1 )";
				break;
			case 7:
				sQLiteCommand.CommandText = "Update ksef Set offline = 2 where (offline=1 and (signedanswerfromficscal = '' or signedanswerfromficscal = 'not') and id=(select id from ksef where (offline=3) LIMIT 1 )+1)";
				break;
			case 8:
				sQLiteCommand.CommandText = "Update ksef SET signedanswerfromficscal='' where doctype<>80";
				break;
			case 9:
				sQLiteCommand.CommandText = "Delete from Sessions;";
				break;
			case 10:
				sQLiteCommand.CommandText = "Delete from fns where used not null;";
				break;
			case 11:
				sQLiteCommand.CommandText = "VACUUM 'main';";
				break;
			default:
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
				return result;
			}
			sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
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

	public TypErr CloseCurrentShift()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		TypErrStr typErrStr = ReturnOpenShift();
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		if (Operators.CompareString(typErrStr.ReturnStr, "0", TextCompare: false) == 0)
		{
			result.errStr = "Немає відкритої зміни.";
			result.errCode = 8;
			return result;
		}
		int num = 0;
		do
		{
			result.errCode = 0;
			result.errStr = "";
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "UPDATE SHIFTS SET DATEEND=datetime(CURRENT_TIMESTAMP, 'localtime') WHERE ID=" + typErrStr.ReturnStr;
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errStr = "Ошибка. Не могу закрыть текущую смену.";
				result.errCode = 1007;
				ProjectData.ClearProjectError();
				goto IL_0108;
			}
			break;
			IL_0108:
			num = checked(num + 1);
		}
		while (num <= 18);
		return result;
	}

	public TypErr SaveCheck(string UID, string TotalSum, string DocType, string TaxNumCheck, string smbS = "0")
	{
		smbS = All.Bablo(smbS);
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		TypErrLLCNshift typErrLLCNshift = ReturnLocalCheckNumberShift();
		if (typErrLLCNshift.errCode > 0)
		{
			result.errCode = typErrLLCNshift.errCode;
			result.errStr = typErrLLCNshift.errStr;
			return result;
		}
		typErrLLCNshift.LastLocalCheckNumbern = Conversions.ToInteger(typErrLLCNshift.LastLocalCheckNumbern).ToString();
		int num = Conversions.ToInteger(MaxID("CHECKHEAD").ReturnStr);
		int num2 = 0;
		checked
		{
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = All.A.Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO CHECKHEAD \r\n            (ORDERDATE, FN, UID, TOTALSUM, CASHIER, ORDERNUM, ORDERTAXNUM, SHIFTID, TIN, INN, ORGNAME, POINTNAME, POINTADDR, DOCTYPE, VER, CASHDESKNUM)\r\n            VALUES \r\n            (datetime(CURRENT_TIMESTAMP, 'localtime'),'" + All.A.FN + "','" + UID + "','" + TotalSum + "','" + typErrLLCNshift.Cashier + "','" + typErrLLCNshift.LastLocalCheckNumbern + "','" + TaxNumCheck + "','" + typErrLLCNshift.IDshift + "','" + All.A.TIN + "','" + All.A.INN + "','" + TextToTextSQL(All.A.OrgName) + "','" + TextToTextSQL(All.A.PointName) + "','" + TextToTextSQL(All.A.PointAddr) + "','" + DocType + "','1','" + smbS + "')";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("CHECKHEAD").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 16;
					result.errStr = "Ошибка при записи в таблицу CHECKHEAD - " + ex2.Message;
					ProjectData.ClearProjectError();
					goto IL_0264;
				}
				break;
				IL_0264:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
	}

	public TypErr SaveCheckPay(string CheckID, string Pay, string S)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		if (All.StrToDouble(S) == 0.0)
		{
			return result;
		}
		string sSQL = "";
		S = All.Bablo(S);
		int num = Conversions.ToInteger(MaxID("CHECKPAY").ReturnStr);
		int num2 = 0;
		do
		{
			result.errCode = 0;
			result.errStr = "";
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO CHECKPAY (CHECKID, PAYMENTFORM, TOTALSUM) VALUES ('" + CheckID + "','" + Pay + "','" + S + "')";
				sSQL = sQLiteCommand.CommandText;
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
				if (num != Conversions.ToInteger(MaxID("CHECKPAY").ReturnStr))
				{
					break;
				}
				result.errCode = 16;
				result.errStr = "Ошибка записи информации о чеке в таблицу CHECKPAY";
				goto IL_0177;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				if (num != Conversions.ToInteger(MaxID("CHECKPAY").ReturnStr))
				{
					ProjectData.ClearProjectError();
					break;
				}
				result.errCode = 16;
				result.errStr = "Ошибка записи информации о чеке в таблицу CHECKPAY - " + ex2.Message;
				ProjectData.ClearProjectError();
				goto IL_0177;
			}
			IL_0177:
			num2 = checked(num2 + 1);
		}
		while (num2 <= 18);
		if (result.errCode > 0)
		{
			ErrorSaveSQL(sSQL);
		}
		return result;
	}

	private void ErrorSaveSQL(string sSQL)
	{
		IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\errors.ini");
		int num = 1;
		do
		{
			if (Operators.CompareString(iniHGB.GetString("ErrorSQL", num.ToString()), "", TextCompare: false) == 0)
			{
				iniHGB.WriteString("ErrorSQL", num.ToString(), sSQL);
				break;
			}
			num = checked(num + 1);
		}
		while (num <= 999);
	}

	public TypErr SaveTaxa(string CheckID, string TaxCode, string TaxPRC, string TaxSUM)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO CHECKTAX \r\n            (CHECKID, TAXCODE, TAXPRC, TAXSUM)\r\n            VALUES \r\n            ('" + CheckID + "','" + TaxCode + "','" + TaxPRC + "','" + TaxSUM + "')";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 16;
			result.errStr = "Ошибка записи информации о чеке в таблицу CHECKTAX";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErr SaveGood(string CheckID, string CODE, string UKTZED, string GOODSNAME, string AMOUNT, string PRICE, string LETTER, string COST)
	{
		AMOUNT = All.KolvoVes(AMOUNT);
		COST = All.Bablo(COST);
		PRICE = All.Bablo(PRICE);
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		int num = Conversions.ToInteger(MaxID("CHECKBODY").ReturnStr);
		int num2 = 0;
		checked
		{
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = All.A.Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO CHECKBODY \r\n            (CHECKID, CODE, UKTZED, GOODSNAME, AMOUNT, PRICE, LETTER, COST )\r\n            VALUES \r\n            ('" + CheckID + "','" + CODE + "','" + UKTZED + "','" + GOODSNAME + "','" + AMOUNT + "','" + PRICE + "','" + LETTER + "','" + COST + "')";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("CHECKBODY").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 16;
					result.errStr = "Ошибка записи информации о чеке в таблицу CHECKTAX";
					ProjectData.ClearProjectError();
					goto IL_0171;
				}
				break;
				IL_0171:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
	}

	public TypErrStr SaveDealCheck(string Sum, string DocType, string PaymentName, string TaxNumCheck)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		TypErr typErr = SaveCheck("", Sum, DocType, TaxNumCheck);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			return result;
		}
		TypErrStr typErrStr = All.l.MaxID("CHECKHEAD");
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		typErr = SaveCheckPay(typErrStr.ReturnStr, PaymentName, Sum);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			return result;
		}
		result.ReturnStr = typErrStr.ReturnStr;
		return result;
	}

	public TypErr SaveXMLcheck(string checkid, string checkxml, string checkXMLnotDot, string signedanswerfromficscal, string checkidficscal, string TypDoc, string SumT = "0.00", string PathFile = "")
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		checkxml = Strings.Replace(checkxml, "'", "\"");
		checkXMLnotDot = Strings.Replace(checkXMLnotDot, "'", "\"");
		checkidficscal = checkidficscal.Replace("`", "_");
		if (Operators.CompareString(TypDoc.Trim(), "80", TextCompare: false) != 0)
		{
			signedanswerfromficscal = "";
		}
		signedanswerfromficscal = Strings.Replace(signedanswerfromficscal, "'", "\"");
		TypErrStr typErrStr = ReturnOpenShift();
		if (typErrStr.errCode > 0)
		{
			All.Lg.SaveTextToLog("SaveXMLcheck", "Внимание! Запись в ksef c ошибочным номером смны: " + typErrStr.ReturnStr, "Ошибка №" + typErrStr.errCode + " - " + typErrStr.errStr);
		}
		string text = SHA.GenerateSHA256File(PathFile);
		string text2 = "0";
		checked
		{
			if (Operators.CompareString(TypDoc, "8", TextCompare: false) == 0)
			{
				text2 = "0";
				checkid = typErrStr.ReturnStr + "A";
			}
			else
			{
				TypErrLLCNshift typErrLLCNshift = ReturnLocalCheckNumberShift();
				if (typErrLLCNshift.errCode > 0)
				{
					All.Lg.SaveTextToLog("SaveXMLcheck", "Внимание! Запись в ksef c ошибочным локальным номером: " + typErrLLCNshift.LastLocalCheckNumbern, "Ошибка №" + typErrLLCNshift.errCode + " - " + typErrLLCNshift.errStr);
					typErrLLCNshift.LastLocalCheckNumbern = "0";
				}
				text2 = (Conversions.ToInteger(typErrLLCNshift.LastLocalCheckNumbern) + 1).ToString();
			}
			int num = Conversions.ToInteger(MaxID("ksef").ReturnStr);
			int num2 = 0;
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = All.A.Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO ksef \r\n            (checkid, checkxml, checksigned, signedanswerfromficscal, checkidficscal, shiftid, DocType, MAC, sum, dt, localchecknumber)\r\n            VALUES \r\n            ('" + checkid + "','" + checkxml + "','" + checkXMLnotDot + "','" + signedanswerfromficscal + "','" + checkidficscal + "', '" + typErrStr.ReturnStr + " ', '" + TypDoc + "', '" + text + "', '" + SumT + "', datetime(CURRENT_TIMESTAMP, 'localtime'), '" + text2 + "' )";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("ksef").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 16;
					result.errStr = "Ошибка записи информации о чеке в таблицу ksef";
					ProjectData.ClearProjectError();
					goto IL_02e9;
				}
				break;
				IL_02e9:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
	}

	public bool OfflineTrue()
	{
		bool result;
		if (Operators.CompareString(All.A.FN, "7000000512", TextCompare: false) == 0)
		{
			result = true;
		}
		else
		{
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "SELECT ID FROM ksef WHERE offline > '1'";
				result = (sQLiteCommand.ExecuteReader().Read() ? true : false);
				((Component)(object)sQLiteCommand).Dispose();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = false;
				ProjectData.ClearProjectError();
			}
		}
		return result;
	}

	internal bool CloseOffline10()
	{
		string text = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT DocType FROM ksef WHERE ID = (SELECT MAX(ID) FROM ksef)";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			text = ((!sQLiteDataReader.Read()) ? "0" : sQLiteDataReader[0].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "0";
			ProjectData.ClearProjectError();
		}
		if (Conversions.ToInteger(text) == 10)
		{
			return true;
		}
		return false;
	}

	internal bool BagCloseOfflineShift()
	{
		string text = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select DT from ksef where (shiftid = (Select id from shifts where DATEEND = 'NULL' LIMIT 1)and DocType = 80 and  offline <> 0)";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			text = ((!sQLiteDataReader.Read()) ? "" : sQLiteDataReader[0].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "";
			ProjectData.ClearProjectError();
		}
		if (text.Length > 0)
		{
			return true;
		}
		return false;
	}

	public TypErrStr OfflineDate()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT dt FROM ksef WHERE offline > '1' and DocType = '9'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnStr = sQLiteDataReader[0].ToString();
			}
			else
			{
				result.ReturnStr = "";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrStr CheckDate(string eID)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT dt FROM ksef WHERE ID = '" + eID + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnStr = sQLiteDataReader[0].ToString();
			}
			else
			{
				result.ReturnStr = "";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public string OfflineTrueInt()
	{
		if (OfflineTrue())
		{
			return "1";
		}
		return "0";
	}

	public int OfflineCheckCount()
	{
		string value = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM ksef WHERE offline = '2'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				value = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			value = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(value);
	}

	public int OfflineOpenID()
	{
		string text = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select ID FROM ksef WHERE offline = '3'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			text = ((!sQLiteDataReader.Read()) ? "0" : sQLiteDataReader[0].ToString());
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(text);
	}

	public TypErr UPDATEksef(string ID, string NameCol, string Infa)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		Infa = Strings.Replace(Infa, "'", "\"");
		int num = 0;
		do
		{
			result.errCode = 0;
			result.errStr = "";
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				SQLiteCommand sQLiteCommand = new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "UPDATE ksef SET " + NameCol + "='" + Infa + "' WHERE ID='" + ID + "'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errStr = "Ошибка при попытке изменить запись в таблице ksef.";
				result.errCode = 40;
				ProjectData.ClearProjectError();
				goto IL_00e2;
			}
			break;
			IL_00e2:
			num = checked(num + 1);
		}
		while (num <= 18);
		return result;
	}

	internal TypErr CloseOfflineKsef()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		int num = 0;
		do
		{
			result.errCode = 0;
			result.errStr = "";
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "UPDATE ksef SET offline='1' WHERE offline='3'";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errStr = "Ошибка при попытке изменить запись в таблице ksef.";
				result.errCode = 40;
				ProjectData.ClearProjectError();
				goto IL_0098;
			}
			break;
			IL_0098:
			num = checked(num + 1);
		}
		while (num <= 18);
		return result;
	}

	public TypErr SaveXMLcheckOffline(string checkid, string checkxml, string checkXMLnotDot, string signedanswerfromficscal, string checkidficscal, string TypDoc, string SumT = "0.00")
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		checkxml = Strings.Replace(checkxml, "'", "\"");
		checkXMLnotDot = Strings.Replace(checkXMLnotDot, "'", "\"");
		checkidficscal = checkidficscal.Replace("`", "_");
		if (Operators.CompareString(TypDoc.Trim(), "80", TextCompare: false) != 0)
		{
			signedanswerfromficscal = "";
		}
		signedanswerfromficscal = Strings.Replace(signedanswerfromficscal, "'", "\"");
		TypErrStr typErrStr = ReturnOpenShift();
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		checked
		{
			if (Conversions.ToInteger(typErrStr.ReturnStr) < 1)
			{
				typErrStr = All.l.MaxID("SHIFTS");
				typErrStr.ReturnStr = (Conversions.ToInteger(typErrStr.ReturnStr) + 1).ToString();
			}
			TypErrStr typErrStr2 = LastMac();
			if (typErrStr2.errCode > 0)
			{
				typErrStr2.ReturnStr = "";
			}
			string xMLcheck = checkXMLnotDot.Replace("mmmaaaccc", "<MAC ID='" + checkidficscal + "'>" + typErrStr2.ReturnStr + "</MAC>");
			string text = MakCheck(xMLcheck);
			string text2 = "0";
			if (Operators.CompareString(TypDoc, "8", TextCompare: false) == 0)
			{
				text2 = "0";
				checkid = typErrStr.ReturnStr + "A";
			}
			else
			{
				TypErrLLCNshift typErrLLCNshift = ReturnLocalCheckNumberShift();
				if (typErrLLCNshift.errCode > 0)
				{
					typErrLLCNshift.LastLocalCheckNumbern = "-1";
				}
				text2 = (Conversions.ToInteger(typErrLLCNshift.LastLocalCheckNumbern) + 1).ToString();
			}
			int num = Conversions.ToInteger(MaxID("ksef").ReturnStr);
			int num2 = 0;
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = All.A.Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO ksef \r\n            (checkid, checkxml, checksigned, signedanswerfromficscal, checkidficscal, shiftid, DocType, MAC, sum, dt, localchecknumber, offline)\r\n            VALUES\r\n            ('" + checkid + "','" + checkxml + "','" + checkXMLnotDot + "','" + signedanswerfromficscal + "','" + checkidficscal + "', '" + typErrStr.ReturnStr + " ', '" + TypDoc + "', '" + text + "', '" + SumT + "', datetime(CURRENT_TIMESTAMP, 'localtime'), '" + text2 + "', '2')";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("ksef").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 16;
					result.errStr = "Помилка запису інформації про офлайн чеку в таблиці ksef";
					ProjectData.ClearProjectError();
					goto IL_0323;
				}
				break;
				IL_0323:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
	}

	public TypErr SaveXMLcheckOfflineTechno(string checkid, string checkxml, string checkXMLnotDot, string signedanswerfromficscal, string checkidficscal, string TypDoc, string SumT = "0.00")
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		checkxml = Strings.Replace(checkxml, "'", "\"");
		checkXMLnotDot = Strings.Replace(checkXMLnotDot, "'", "\"");
		checkidficscal = checkidficscal.Replace("`", "_");
		if (Operators.CompareString(TypDoc.Trim(), "80", TextCompare: false) != 0)
		{
			signedanswerfromficscal = "";
		}
		signedanswerfromficscal = Strings.Replace(signedanswerfromficscal, "'", "\"");
		TypErrStr typErrStr = ReturnOpenShift();
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		checked
		{
			if (Conversions.ToInteger(typErrStr.ReturnStr) < 1)
			{
				typErrStr = All.l.MaxID("SHIFTS");
				typErrStr.ReturnStr = (Conversions.ToInteger(typErrStr.ReturnStr) + 1).ToString();
			}
			TypErrStr typErrStr2 = LastMac();
			if (typErrStr2.errCode > 0)
			{
				typErrStr2.ReturnStr = "";
			}
			string xMLcheck = checkXMLnotDot.Replace("mmmaaaccc", "<MAC ID='" + checkidficscal + "'>" + typErrStr2.ReturnStr + "</MAC>");
			string text = MakCheck(xMLcheck);
			string text2 = "0";
			if (Operators.CompareString(TypDoc, "8", TextCompare: false) == 0)
			{
				text2 = "0";
				checkid = typErrStr.ReturnStr + "A";
			}
			else
			{
				TypErrLLCNshift typErrLLCNshift = ReturnLocalCheckNumberShift();
				if (typErrLLCNshift.errCode > 0)
				{
					typErrLLCNshift.LastLocalCheckNumbern = "-1";
				}
				text2 = (Conversions.ToInteger(typErrLLCNshift.LastLocalCheckNumbern) + 1).ToString();
			}
			int num = Conversions.ToInteger(MaxID("ksef").ReturnStr);
			int num2 = 0;
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = All.A.Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO ksef \r\n            (checkid, checkxml, checksigned, signedanswerfromficscal, checkidficscal, shiftid, DocType, MAC, sum, dt, localchecknumber, offline)\r\n            VALUES\r\n            ('" + checkid + "','" + checkxml + "','" + checkXMLnotDot + "','" + signedanswerfromficscal + "','" + checkidficscal + "', '" + typErrStr.ReturnStr + " ', '" + TypDoc + "', '" + text + "', '" + SumT + "', datetime(CURRENT_TIMESTAMP, 'localtime'), '" + text2 + "', '3')";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("ksef").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 16;
					result.errStr = "Помилка запису інформації про офлайн чеку в таблиці ksef";
					ProjectData.ClearProjectError();
					goto IL_0323;
				}
				break;
				IL_0323:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
	}

	private string MakCheck(string XMLcheck)
	{
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\LastMAK.xml";
		All.SaveToFileText(text, XMLcheck);
		return SHA.GenerateSHA256File(text);
	}

	public TypErr SaveShiftAll(string TaxNumCheck, string DocType)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		TypErrLLCNshift typErrLLCNshift = ReturnLocalCheckNumberShift();
		if (typErrLLCNshift.errCode > 0)
		{
			result.errCode = typErrLLCNshift.errCode;
			result.errStr = typErrLLCNshift.errStr;
			return result;
		}
		int num = Conversions.ToInteger(MaxID("CHECKHEAD").ReturnStr);
		int num2 = 0;
		checked
		{
			do
			{
				result.errCode = 0;
				result.errStr = "";
				try
				{
					SQLiteConnection sQLiteConnection = new SQLiteConnection();
					SQLiteCommand sQLiteCommand = new SQLiteCommand();
					sQLiteConnection.ConnectionString = All.A.Connection;
					sQLiteConnection.Open();
					sQLiteCommand = sQLiteConnection.CreateCommand();
					sQLiteCommand.CommandText = "INSERT INTO CHECKHEAD \r\n            (ORDERDATE, FN, CASHIER, ORDERNUM, ORDERTAXNUM, SHIFTID, TIN, INN, ORGNAME, POINTNAME, POINTADDR, DOCTYPE, VER )\r\n            VALUES \r\n            (datetime(CURRENT_TIMESTAMP, 'localtime'),'" + All.A.FN + "','" + typErrLLCNshift.Cashier + "','" + typErrLLCNshift.LastLocalCheckNumbern + "','" + TaxNumCheck + "','" + typErrLLCNshift.IDshift + "','" + All.A.TIN + "','" + All.A.INN + "','" + All.A.OrgName + "','" + All.A.PointName + "','" + All.A.PointAddr + "','" + DocType + "','1')";
					SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
					((Component)(object)sQLiteCommand).Dispose();
					sQLiteDataReader.Close();
					sQLiteConnection.Close();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (num + 1 == Conversions.ToInteger(MaxID("CHECKHEAD").ReturnStr))
					{
						ProjectData.ClearProjectError();
						break;
					}
					result.errCode = 29;
					result.errStr = "Ошибка записи действия в таблицу CHECKHEAD";
					ProjectData.ClearProjectError();
					goto IL_01f4;
				}
				break;
				IL_01f4:
				num2++;
			}
			while (num2 <= 18);
			return result;
		}
	}

	public TypErrStr LastMac()
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		TypErrStr result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE ID = (SELECT MAX(ID) FROM ksef)";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			typErrStr.ReturnStr = sQLiteDataReader[8].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			typErrStr.errCode = 30;
			typErrStr.errStr = "Ошибка получения предыдущего МАК";
			typErrStr.ReturnStr = "";
			result = typErrStr;
			ProjectData.ClearProjectError();
			goto IL_00ec;
		}
		if (Operators.CompareString(typErrStr.ReturnStr, "", TextCompare: false) == 0)
		{
			typErrStr.errCode = 30;
			typErrStr.errStr = "Ошибка получения предыдущего МАК";
			typErrStr.ReturnStr = "";
		}
		result = typErrStr;
		goto IL_00ec;
		IL_00ec:
		return result;
	}

	public TypErrKsef CheckForSend()
	{
		TypErrKsef result = default(TypErrKsef);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnXML = "";
		result.ReturnID = "";
		result.ReturnLocalNumber = "";
		result.ReturnShift = "";
		result.ReturnNumber = "";
		result.ReturnTyp = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM ksef WHERE offline='2'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ReturnXML = sQLiteDataReader[2].ToString();
				result.ReturnID = sQLiteDataReader[11].ToString();
				result.ReturnLocalNumber = sQLiteDataReader[5].ToString();
				result.ReturnShift = sQLiteDataReader[9].ToString();
				result.ReturnNumber = sQLiteDataReader[4].ToString();
				result.ReturnTyp = sQLiteDataReader[6].ToString();
			}
			else
			{
				result.errCode = 38;
				result.errStr = "Ошибка чтения данных из таблицы ksef";
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 38;
			result.errStr = "Ошибка получение данных из таблицы ksef";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public TypErrStr NumberTaxIDksef(string id)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT checkidficscal FROM ksef WHERE ID='" + id + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			sQLiteDataReader.Read();
			result.ReturnStr = sQLiteDataReader[0].ToString();
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "0";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.ReturnStr, "", TextCompare: false) == 0)
		{
			result.ReturnStr = "0";
		}
		return result;
	}

	public bool AddNewOperator(string FioS, string PathKS, string PassS, string InnS)
	{
		int num = 0;
		bool result;
		try
		{
			PassS = new Coding().Cod(PassS.Trim());
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "INSERT INTO OPERATORS \r\n            (OPERATORNAME, KEYPATH, KEYPASS, INN )\r\n            VALUES \r\n            ('" + FioS + "', '" + PathKS + "', '" + PassS + "', '" + InnS + "' )";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
			result = true;
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

	public bool UpdateOperator(string FioS, string PathKS, string PassS, string InnS)
	{
		int num = 0;
		bool result;
		try
		{
			PassS = new Coding().Cod(PassS.Trim());
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE OPERATORS SET OPERATORNAME = '" + FioS + "', KEYPATH = '" + PathKS + "', KEYPASS = '" + PassS + "' WHERE INN='" + InnS + "';";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
			result = true;
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

	internal int SearchPayFormsID(string NamePay)
	{
		int num = 0;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM PayForms WHERE NAME ='" + NamePay + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			num = (sQLiteDataReader.Read() ? Conversions.ToInteger(sQLiteDataReader[0]) : 0);
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			num = 0;
			ProjectData.ClearProjectError();
		}
		return num;
	}

	public int CountUID(string uidS)
	{
		string value = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM CHECKHEAD WHERE UID = '" + uidS + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				value = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			value = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(value);
	}

	public TypShiftCheckID UidToTaxId(string eUID)
	{
		TypShiftCheckID result = default(TypShiftCheckID);
		result.CheckID = "";
		result.ShiftID = "";
		eUID = eUID.Trim();
		if (Operators.CompareString(eUID, "", TextCompare: false) == 0)
		{
			return result;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select SHIFTID, ORDERTAXNUM FROM CHECKHEAD WHERE UID = '" + eUID + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.ShiftID = sQLiteDataReader[0].ToString();
				result.CheckID = sQLiteDataReader[1].ToString();
			}
			else
			{
				result.CheckID = "";
				result.ShiftID = "";
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ShiftID = "";
			result.ShiftID = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal bool BlockBase(bool bl)
	{
		if (!bl)
		{
			EndMyBlock();
			BlockLoсal = false;
			return true;
		}
		checked
		{
			if (!All.A.Multiplayer)
			{
				EndMyBlock();
				if (!BlockLoсal)
				{
					BlockLoсal = true;
					return true;
				}
				int num = 1;
				do
				{
					if (!BlockLoсal)
					{
						BlockLoсal = true;
						return true;
					}
					Thread.Sleep(333);
					num++;
				}
				while (num <= 999);
				BlockLoсal = true;
				return true;
			}
			int num2 = 1;
			TypErrInt typErrInt;
			do
			{
				typErrInt = SetBlockS();
				if (typErrInt.errCode == 0)
				{
					break;
				}
				if (typErrInt.errCode == 61)
				{
					NumberBlock = 0L;
				}
				else if (typErrInt.errCode > 0)
				{
					NumberBlock = 0L;
					return false;
				}
				num2++;
			}
			while (num2 <= 9);
			if (NumberBlock > 0)
			{
				if (typErrInt.ReturnInt == NumberBlock)
				{
					return true;
				}
				NumberBlock = typErrInt.ReturnInt;
				return true;
			}
			NumberBlock = typErrInt.ReturnInt;
			return true;
		}
	}

	private TypErrInt SetBlockS()
	{
		TypErrInt result = default(TypErrInt);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnInt = 0L;
		int num = 1;
		checked
		{
			do
			{
				result.errCode = 0;
				result.errStr = "";
				result.ReturnInt = 0L;
				TypErrBlock blockS = GetBlockS();
				if (blockS.id > 0)
				{
					if (blockS.id == NumberBlock)
					{
						if (Math.Abs((int)DateAndTime.DateDiff(DateInterval.Second, blockS.SessionStartDT, DateTime.Now)) < 63)
						{
							result.ReturnInt = blockS.id;
							return result;
						}
					}
					else
					{
						NumberBlock = 0L;
					}
					result.errCode = 58;
					result.errStr = "Блокировка установленна ранее";
					if (Math.Abs((int)DateAndTime.DateDiff(DateInterval.Second, blockS.SessionStartDT, DateTime.Now)) > 81)
					{
						UpdateBlockStatus();
						result.errCode = 0;
						result.errStr = "";
						result.ReturnInt = 0L;
						break;
					}
					Thread.Sleep(333);
					num++;
					continue;
				}
				result.errCode = 0;
				result.errStr = "";
				result.ReturnInt = 0L;
				break;
			}
			while (num <= 999);
			if (result.errCode > 0)
			{
				return result;
			}
			try
			{
				SQLiteConnection sQLiteConnection = new SQLiteConnection();
				new SQLiteCommand();
				sQLiteConnection.ConnectionString = All.A.Connection;
				sQLiteConnection.Open();
				SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
				sQLiteCommand.CommandText = "INSERT INTO Sessions (SessionStartDT, SessionStatus) VALUES (datetime(CURRENT_TIMESTAMP, 'localtime'), '1')";
				SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
				((Component)(object)sQLiteCommand).Dispose();
				sQLiteDataReader.Close();
				sQLiteConnection.Close();
				result.ReturnInt = GetBlockS().id;
				if (result.ReturnInt < 1)
				{
					result.errCode = 57;
					result.errStr = "Ошибка при записи в таблицу Sessions.";
					result.ReturnInt = 0L;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result.errCode = 57;
				result.errStr = "Ошибка при записи в таблицу Sessions.";
				result.ReturnInt = 0L;
				ProjectData.ClearProjectError();
			}
			Application.DoEvents();
			if (BlockCount() > 1)
			{
				EndMyBlock();
				result.errCode = 61;
				result.errStr = "Ошибка блокировок. Открыто более одной сессии.";
				result.ReturnInt = 0L;
			}
			return result;
		}
	}

	private TypErrBlock GetBlockS()
	{
		TypErrBlock result = default(TypErrBlock);
		result.errCode = 0;
		result.errStr = "";
		result.id = 0L;
		result.SessionStartDT = DateTime.Now;
		result.SessionStatus = 0;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * FROM Sessions WHERE SessionStatus = '1'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.id = Conversions.ToInteger(sQLiteDataReader[0].ToString());
				result.SessionStartDT = Convert.ToDateTime(sQLiteDataReader[1].ToString());
				result.SessionStatus = Conversions.ToInteger(sQLiteDataReader[2].ToString());
			}
			((Component)(object)sQLiteCommand).Dispose();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 56;
			result.errStr = "Ошибка при получении блокировок";
			result.id = 0L;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private TypErr UpdateBlockStatus(int StatusOld = 1, int StatusNew = 0)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE Sessions SET SessionStatus ='" + StatusNew + "' WHERE SessionStatus='" + StatusOld + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка при попытке изменить запись в таблице Sessions.";
			result.errCode = 59;
			ProjectData.ClearProjectError();
		}
		NumberBlock = 0L;
		return result;
	}

	private TypErr EndMyBlock(int StatusNew = 0)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		if (NumberBlock < 1)
		{
			NumberBlock = 0L;
			return result;
		}
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "UPDATE Sessions SET SessionStatus ='" + StatusNew + "' WHERE id='" + NumberBlock + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errStr = "Ошибка при попытке изменить запись в таблице Sessions.";
			result.errCode = 59;
			ProjectData.ClearProjectError();
		}
		NumberBlock = 0L;
		return result;
	}

	public int BlockCount()
	{
		string value = "0";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select COUNT(*) FROM Sessions WHERE SessionStatus = '1'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				value = sQLiteDataReader[0].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			value = "0";
			ProjectData.ClearProjectError();
		}
		return Conversions.ToInteger(value);
	}

	private string ConnectPathBackup()
	{
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db";
		return "Data Source=" + text + "; Version=3";
	}

	public void ClearBackups()
	{
		ClearBackupTable();
		int num = 0;
		do
		{
			ClearBackup(num);
			num = checked(num + 1);
		}
		while (num <= 26);
	}

	private void ClearBackup(int nt)
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			SQLiteCommand sQLiteCommand = new SQLiteCommand();
			sQLiteConnection.ConnectionString = ConnectPathBackup();
			sQLiteConnection.Open();
			sQLiteCommand = sQLiteConnection.CreateCommand();
			switch (nt)
			{
			case 0:
				sQLiteCommand.CommandText = "DROP TABLE 'main'.'backuplog';";
				break;
			case 1:
				sQLiteCommand.CommandText = "delete from Sessions;";
				break;
			case 2:
				sQLiteCommand.CommandText = "delete from CHECKPAY;";
				break;
			case 3:
				sQLiteCommand.CommandText = "delete from CHECKTAX;";
				break;
			case 4:
				sQLiteCommand.CommandText = "delete from CHECKBODY;";
				break;
			case 5:
				sQLiteCommand.CommandText = "delete from CHECKHEAD;";
				break;
			case 6:
				sQLiteCommand.CommandText = "update ksef set checksigned='',signedanswerfromficscal='',mac='';";
				break;
			case 7:
				sQLiteCommand.CommandText = "VACUUM 'main';";
				break;
			case 8:
				sQLiteCommand.CommandText = "DROP TRIGGER OPERATORS1;";
				break;
			case 9:
				sQLiteCommand.CommandText = "DROP TRIGGER OPERATORS2;";
				break;
			case 10:
				sQLiteCommand.CommandText = "DROP TRIGGER shifts1;";
				break;
			case 11:
				sQLiteCommand.CommandText = "DROP TRIGGER shifts2;";
				break;
			case 12:
				sQLiteCommand.CommandText = "DROP TRIGGER shifts3;";
				break;
			case 13:
				sQLiteCommand.CommandText = "DROP TRIGGER checkbody1;";
				break;
			case 14:
				sQLiteCommand.CommandText = "DROP TRIGGER checkhead1;";
				break;
			case 15:
				sQLiteCommand.CommandText = "DROP TRIGGER checkpay1;";
				break;
			case 16:
				sQLiteCommand.CommandText = "DROP TRIGGER checktax1;";
				break;
			case 17:
				sQLiteCommand.CommandText = "DROP TRIGGER ksefup1;";
				break;
			case 18:
				sQLiteCommand.CommandText = "DROP TRIGGER ksefup2;";
				break;
			case 19:
				sQLiteCommand.CommandText = "DROP TRIGGER ksefup3;";
				break;
			case 20:
				sQLiteCommand.CommandText = "DROP TRIGGER ksefup4;";
				break;
			case 21:
				sQLiteCommand.CommandText = "DROP TRIGGER payforms1;";
				break;
			case 22:
				sQLiteCommand.CommandText = "DROP TRIGGER payforms2;";
				break;
			case 23:
				sQLiteCommand.CommandText = "DROP TRIGGER taxob;";
				break;
			case 24:
				sQLiteCommand.CommandText = "DROP TRIGGER taxobj2;";
				break;
			case 25:
				sQLiteCommand.CommandText = "UPDATE OPERATORS SET KEYPASS=''";
				break;
			case 26:
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

	private void ClearBackupTable()
	{
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "delete from backuplog; VACUUM 'main';";
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

	public TypBackupInfo InfoBackup()
	{
		TypBackupInfo result = default(TypBackupInfo);
		result.First = "нема записів";
		result.Last = "нема записів";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = ConnectPathBackup();
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "Select max (dt) AS maxdt ,min(dt) as mindt  from ksef;";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.Last = sQLiteDataReader[0].ToString();
				result.First = sQLiteDataReader[1].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.First = "---";
			result.Last = "---";
			ProjectData.ClearProjectError();
		}
		if (Operators.CompareString(result.First, "", TextCompare: false) == 0)
		{
			result.First = "нема записів";
		}
		if (Operators.CompareString(result.Last, "", TextCompare: false) == 0)
		{
			result.Last = "нема записів";
		}
		return result;
	}

	public void TransferBackup()
	{
		if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0 || !File.Exists(All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db"))
		{
			return;
		}
		All.l.ClearBackup(25);
		int num = 0;
		do
		{
			TypTransferBackup typTransferBackup = sqlBackup();
			if (Operators.CompareString(typTransferBackup.id, "", TextCompare: false) != 0)
			{
				if (sqlForBackup(typTransferBackup.sql))
				{
					DelSqlBackup(typTransferBackup.id);
				}
				else if (sqlForBackup(NewSql(typTransferBackup.sql)))
				{
					DelSqlBackup(typTransferBackup.id);
				}
				num = checked(num + 1);
				continue;
			}
			break;
		}
		while (num <= 27);
	}

	internal bool TableKsef()
	{
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = ConnectPathBackup();
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT name FROM sqlite_master WHERE type='table' AND name='ksef';";
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

	internal bool TableBackuplog()
	{
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT name FROM sqlite_master WHERE type='table' AND name='backuplog';";
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

	private TypTransferBackup sqlBackup()
	{
		TypTransferBackup result = default(TypTransferBackup);
		result.id = "";
		result.sql = "";
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT * from backuplog where cmt is null LIMIT 1";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			if (sQLiteDataReader.Read())
			{
				result.id = sQLiteDataReader[0].ToString();
				result.sql = sQLiteDataReader[1].ToString();
			}
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.id = "";
			result.sql = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private bool sqlForBackup(string sqlS)
	{
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = ConnectPathBackup();
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = sqlS;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			All.Lg.SaveTextToLog("Ошибка переноса SQL запроса", sqlS);
			result = false;
			ProjectData.ClearProjectError();
			goto IL_006a;
		}
		result = true;
		goto IL_006a;
		IL_006a:
		return result;
	}

	internal string NewSql(string eSQL)
	{
		string text = TagSql(eSQL, "id");
		string text2 = TagSql(eSQL, "name");
		string text3 = TagSql(eSQL, "iscash");
		return "UPDATE PayForms SET name=" + text2 + ", iscash=" + text3 + " where id=" + text;
	}

	internal string TagSql(string eSQL, string eTag, bool forSQL = true)
	{
		int num = eSQL.IndexOf(eTag);
		if (num < 0)
		{
			return "";
		}
		string text = "";
		bool flag = false;
		checked
		{
			int num2 = eSQL.Length - 1;
			for (int i = num; i <= num2; i++)
			{
				string text2 = Conversions.ToString(eSQL[i]);
				if (flag)
				{
					if (Operators.CompareString(text2, "'", TextCompare: false) == 0)
					{
						if (forSQL)
						{
							return "'" + text + "'";
						}
						return text;
					}
					text += text2;
				}
				if (Operators.CompareString(text2, "'", TextCompare: false) == 0)
				{
					flag = true;
				}
			}
			return "";
		}
	}

	private bool DelSqlBackup(string id)
	{
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "delete from backuplog where id='" + id + "'";
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			All.Lg.SaveTextToLog("Ошибка удаления строки id", id);
			result = false;
			ProjectData.ClearProjectError();
			goto IL_007d;
		}
		result = true;
		goto IL_007d;
		IL_007d:
		return result;
	}

	internal bool BackUpLog(string request)
	{
		request = Strings.Replace(request, "'", "''");
		string text = "INSERT INTO backuplog (tobackup)  VALUES ('" + request + "');";
		bool result;
		try
		{
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = All.A.Connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = text;
			SQLiteDataReader sQLiteDataReader = sQLiteCommand.ExecuteReader();
			((Component)(object)sQLiteCommand).Dispose();
			sQLiteDataReader.Close();
			sQLiteConnection.Close();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			All.Lg.SaveTextToLog("request", text, ex2.Message);
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0095;
		}
		result = true;
		goto IL_0095;
		IL_0095:
		return result;
	}

	internal bool OST(string eMinutes)
	{
		bool result;
		try
		{
			string connection = All.A.Connection;
			SQLiteConnection sQLiteConnection = new SQLiteConnection();
			new SQLiteCommand();
			sQLiteConnection.ConnectionString = connection;
			sQLiteConnection.Open();
			SQLiteCommand sQLiteCommand = sQLiteConnection.CreateCommand();
			sQLiteCommand.CommandText = "SELECT datebeg FROM shifts where (dateend = 'NULL' and datebeg < datetime(date('now'),'" + eMinutes + " minutes'))";
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
}
