using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
public class FormTestErrors : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("CopyResult")]
	private Button _CopyResult;

	[CompilerGenerated]
	[AccessedThroughProperty("РозпочатиТестуванняToolStripMenuItem")]
	private ToolStripMenuItem _РозпочатиТестуванняToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ОновитиСертифікатиToolStripMenuItem")]
	private ToolStripMenuItem _ОновитиСертифікатиToolStripMenuItem;

	private bool CloseTest;

	private bool StopTest;

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				noB.Click -= value2;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				noB.Click += value2;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				okB.Click -= value2;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				okB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("TestResult")]
	internal virtual TextBox TestResult
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button CopyResult
	{
		[CompilerGenerated]
		get
		{
			return _CopyResult;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CopyResult_Click;
			Button copyResult = _CopyResult;
			if (copyResult != null)
			{
				copyResult.Click -= value2;
			}
			_CopyResult = value;
			copyResult = _CopyResult;
			if (copyResult != null)
			{
				copyResult.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("MenuStrip1")]
	internal virtual MenuStrip MenuStrip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("МенюToolStripMenuItem")]
	internal virtual ToolStripMenuItem МенюToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem РозпочатиТестуванняToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _РозпочатиТестуванняToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = РозпочатиТестуванняToolStripMenuItem_Click;
			ToolStripMenuItem розпочатиТестуванняToolStripMenuItem = _РозпочатиТестуванняToolStripMenuItem;
			if (розпочатиТестуванняToolStripMenuItem != null)
			{
				розпочатиТестуванняToolStripMenuItem.Click -= value2;
			}
			_РозпочатиТестуванняToolStripMenuItem = value;
			розпочатиТестуванняToolStripMenuItem = _РозпочатиТестуванняToolStripMenuItem;
			if (розпочатиТестуванняToolStripMenuItem != null)
			{
				розпочатиТестуванняToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ОновитиСертифікатиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОновитиСертифікатиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ОновитиСертифікатиToolStripMenuItem_Click;
			ToolStripMenuItem оновитиСертифікатиToolStripMenuItem = _ОновитиСертифікатиToolStripMenuItem;
			if (оновитиСертифікатиToolStripMenuItem != null)
			{
				оновитиСертифікатиToolStripMenuItem.Click -= value2;
			}
			_ОновитиСертифікатиToolStripMenuItem = value;
			оновитиСертифікатиToolStripMenuItem = _ОновитиСертифікатиToolStripMenuItem;
			if (оновитиСертифікатиToolStripMenuItem != null)
			{
				оновитиСертифікатиToolStripMenuItem.Click += value2;
			}
		}
	}

	public FormTestErrors()
	{
		base.Load += FormTestErrors_Load;
		base.Closing += FormTestErrors_Closing;
		CloseTest = true;
		StopTest = false;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormTestErrors));
		this.NoB = new System.Windows.Forms.Button();
		this.OkB = new System.Windows.Forms.Button();
		this.TestResult = new System.Windows.Forms.TextBox();
		this.CopyResult = new System.Windows.Forms.Button();
		this.MenuStrip1 = new System.Windows.Forms.MenuStrip();
		this.МенюToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ОновитиСертифікатиToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.РозпочатиТестуванняToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem1 = new System.Windows.Forms.ToolStripSeparator();
		this.MenuStrip1.SuspendLayout();
		base.SuspendLayout();
		this.NoB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoB.Location = new System.Drawing.Point(258, 568);
		this.NoB.Name = "NoB";
		this.NoB.Size = new System.Drawing.Size(283, 37);
		this.NoB.TabIndex = 14;
		this.NoB.Text = "Скасувати тестування";
		this.NoB.UseVisualStyleBackColor = true;
		this.OkB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OkB.Location = new System.Drawing.Point(556, 568);
		this.OkB.Name = "OkB";
		this.OkB.Size = new System.Drawing.Size(283, 37);
		this.OkB.TabIndex = 13;
		this.OkB.Text = "Розпочати тестування";
		this.OkB.UseVisualStyleBackColor = true;
		this.TestResult.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TestResult.Location = new System.Drawing.Point(12, 46);
		this.TestResult.Multiline = true;
		this.TestResult.Name = "TestResult";
		this.TestResult.ReadOnly = true;
		this.TestResult.ScrollBars = System.Windows.Forms.ScrollBars.Vertical;
		this.TestResult.Size = new System.Drawing.Size(827, 511);
		this.TestResult.TabIndex = 12;
		this.TestResult.TabStop = false;
		this.CopyResult.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CopyResult.Location = new System.Drawing.Point(12, 568);
		this.CopyResult.Name = "CopyResult";
		this.CopyResult.Size = new System.Drawing.Size(222, 37);
		this.CopyResult.TabIndex = 15;
		this.CopyResult.Text = "У буфер обміну";
		this.CopyResult.UseVisualStyleBackColor = true;
		this.MenuStrip1.ImageScalingSize = new System.Drawing.Size(20, 20);
		this.MenuStrip1.Items.AddRange(new System.Windows.Forms.ToolStripItem[1] { this.МенюToolStripMenuItem });
		this.MenuStrip1.Location = new System.Drawing.Point(0, 0);
		this.MenuStrip1.Name = "MenuStrip1";
		this.MenuStrip1.Size = new System.Drawing.Size(853, 28);
		this.MenuStrip1.TabIndex = 16;
		this.MenuStrip1.Text = "MenuStrip1";
		this.МенюToolStripMenuItem.DropDownItems.AddRange(new System.Windows.Forms.ToolStripItem[3] { this.РозпочатиТестуванняToolStripMenuItem, this.ToolStripMenuItem1, this.ОновитиСертифікатиToolStripMenuItem });
		this.МенюToolStripMenuItem.Name = "МенюToolStripMenuItem";
		this.МенюToolStripMenuItem.Size = new System.Drawing.Size(65, 24);
		this.МенюToolStripMenuItem.Text = "Меню";
		this.ОновитиСертифікатиToolStripMenuItem.Name = "ОновитиСертифікатиToolStripMenuItem";
		this.ОновитиСертифікатиToolStripMenuItem.Size = new System.Drawing.Size(245, 26);
		this.ОновитиСертифікатиToolStripMenuItem.Text = "Оновити сертифікати";
		this.РозпочатиТестуванняToolStripMenuItem.Name = "РозпочатиТестуванняToolStripMenuItem";
		this.РозпочатиТестуванняToolStripMenuItem.Size = new System.Drawing.Size(245, 26);
		this.РозпочатиТестуванняToolStripMenuItem.Text = "Розпочати тестування";
		this.ToolStripMenuItem1.Name = "ToolStripMenuItem1";
		this.ToolStripMenuItem1.Size = new System.Drawing.Size(242, 6);
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(853, 613);
		base.Controls.Add(this.CopyResult);
		base.Controls.Add(this.NoB);
		base.Controls.Add(this.OkB);
		base.Controls.Add(this.TestResult);
		base.Controls.Add(this.MenuStrip1);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MainMenuStrip = this.MenuStrip1;
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormTestErrors";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Пошук та усунення несправностей";
		this.MenuStrip1.ResumeLayout(false);
		this.MenuStrip1.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormTestErrors_Load(object sender, EventArgs e)
	{
		OkB.Enabled = true;
		NoB.Enabled = false;
		TestResult.Text = "- Підключено фіскальний номер " + All.A.FN;
		Text += "           ( WebCheck0.dll  v.6.0.8.1368 )";
	}

	private void FormTestErrors_Closing(object sender, CancelEventArgs e)
	{
		if (!OkB.Enabled)
		{
			StopTest = true;
			NoB.Enabled = false;
			CloseTest = true;
			e.Cancel = true;
		}
		else
		{
			e.Cancel = false;
		}
	}

	private void CopyResult_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(TestResult.Text);
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		TestStart();
	}

	private void TestStart()
	{
		CloseTest = false;
		МенюToolStripMenuItem.Enabled = false;
		StopTest = false;
		OkB.Enabled = false;
		NoB.Enabled = true;
		StartTestErrors();
		OkB.Enabled = true;
		NoB.Enabled = false;
		МенюToolStripMenuItem.Enabled = true;
		CloseTest = true;
	}

	private bool SaveLastCheck()
	{
		bool result;
		try
		{
			TypPrintChecks typPrintChecks = new Reports().CheckXMLlast();
			string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\000.xml";
			if (File.Exists(text))
			{
				FileSystem.DeleteFile(text);
				Application.DoEvents();
			}
			All.SaveToFileText(text, typPrintChecks.ReturnStr);
			Application.DoEvents();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0063;
		}
		result = true;
		goto IL_0063;
		IL_0063:
		return result;
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		StopTest = true;
		NoB.Enabled = false;
	}

	private void StartTestErrors()
	{
		StringBuilder stringBuilder = new StringBuilder();
		TestResult.Text = "";
		Application.DoEvents();
		stringBuilder.Append("- Підключено фіскальний номер " + All.A.FN + " - Початок тесту!");
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Адреса торгової точки: " + All.A.PointAddr);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Число рядків  Ksef: " + All.l.CountMax("ksef").ReturnStr + "   CheckHead: " + All.l.CountMax("CHECKHEAD").ReturnStr + "   Shifts: " + All.l.CountMax("SHIFTS").ReturnStr);
		if (All.A.FullVersion)
		{
			if (All.l.OfflineTrue())
			{
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Статус: офлайн");
				string text = All.l.OfflineDate().ReturnStr.Trim();
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Дата старту: " + text);
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Залишок офлайн чеків: " + All.l.OfflineCheckCount());
				NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Кількість резервних офлайн номерів: " + numbersOfflineUse.CountNubmers());
				string text2 = All.f.StringGetFn(All.A.FN, "LastOfflineErr");
				if (Operators.CompareString(text2.Trim(), "", TextCompare: false) == 0)
				{
					text2 = "відсутня";
				}
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Остання помилка: " + text2);
			}
			else
			{
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Статус: онлайн");
			}
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Безкоштовна версія!");
		}
		stringBuilder.Append("\r\n");
		string text3;
		try
		{
			text3 = "версія: " + FileVersionInfo.GetVersionInfo("C:\\Program Files (x86)\\WebCheck\\PRRO32\\WebCheck.dll").FileVersion + "xxx";
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text3 = "не встановлено";
			ProjectData.ClearProjectError();
		}
		string text4;
		try
		{
			text4 = "версія: " + FileVersionInfo.GetVersionInfo("C:\\Program Files (x86)\\WebCheck\\PRRO64\\WebCheck.dll").FileVersion + "xxx";
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			text4 = "не встановлено";
			ProjectData.ClearProjectError();
		}
		stringBuilder.Append("- PRRO32\\WebCheck.dll " + text3 + "       PRRO64\\WebCheck.dll " + text4);
		string @string = All.f.GetString("Global", "grpcproxy");
		if (Operators.CompareString(@string, "", TextCompare: false) != 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- grpc прокси: " + @string);
		}
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.CountMax("ksef").ReturnStr, "0", TextCompare: false) == 0)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Увага! Таблиця KSEF не містить записів. Перевірка перервана!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 1: Перевірка змін...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.ReturnOpenShift().ReturnStr, "-1", TextCompare: false) == 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(1);
			All.l.BugFix(2);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 2: Перевірка помилки зміни в онлайн...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		bool flag = false;
		if (All.l.OfflineTrue() && Operators.CompareString(LastErrorN(), "-15", TextCompare: false) == 0 && All.l.TestBug(1).ReturnStr.Length > 0)
		{
			flag = true;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (flag)
		{
			All.l.BugFix(3);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 3: Перевірка коректного закриття зміни...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (All.l.TestBug(2).ReturnStr.Length > 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(4);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 4: Перевірка змін №2...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.TestBug(3).ReturnStr, "0", TextCompare: false) != 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(5);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 5: Перевірка закриття змін №2...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.TestBug(4).ReturnStr, "0", TextCompare: false) != 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(6);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 6: Перевірка помилкової позначки офлайн чека");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- OK!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 7: Очищення та стиснення бази даних");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (File.Exists(All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db"))
		{
			All.l.BugFix(8);
			All.l.BugFix(9);
			All.l.BugFix(10);
			All.l.BugFix(11);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Success!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Увага! Очищення та стиснення бази можливе лише при включеному бекапі!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			OkB.Enabled = true;
			NoB.Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 8: Створення файлу останнього чека з бази даних");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (SaveLastCheck())
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ОК!");
			TestResult.Text = stringBuilder.ToString();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
		}
		Application.DoEvents();
		OkB.Enabled = true;
		NoB.Enabled = false;
		StopTest = false;
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Перевірка виконана!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (CloseTest)
		{
			Close();
		}
	}

	private string LastErrorN()
	{
		string text = All.f.StringGetFn(All.A.FN, "LastOfflineErr").Trim();
		if (text.Length < 43)
		{
			return "";
		}
		return Conversions.ToString(text[40]) + Conversions.ToString(text[41]) + Conversions.ToString(text[42]);
	}

	private void РозпочатиТестуванняToolStripMenuItem_Click(object sender, EventArgs e)
	{
		TestStart();
	}

	private void ОновитиСертифікатиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		CloseTest = false;
		МенюToolStripMenuItem.Enabled = false;
		OkB.Enabled = false;
		SertUpLoad();
		МенюToolStripMenuItem.Enabled = true;
		OkB.Enabled = true;
		CloseTest = true;
	}

	private void SertUpLoad()
	{
		string address = "http://lic.webchek.com.ua/prro-tax-gov-ua-chain.lic";
		string text = All.MyDoc() + "\\WebCheck\\prro-tax-gov-ua-chain.tmp";
		string text2 = All.MyDoc() + "\\WebCheck\\prro-tax-gov-ua-chain.pem";
		string newName = "prro-tax-gov-ua-chain.pem";
		string address2 = "http://lic.webchek.com.ua/CACertificates.lic";
		string text3 = All.MyDoc() + "\\WebCheck\\keys\\CACertificates.tmp";
		string text4 = All.MyDoc() + "\\WebCheck\\keys\\CACertificates.p7b";
		string newName2 = "CACertificates.p7b";
		StringBuilder stringBuilder = new StringBuilder();
		TestResult.Text = "";
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		stringBuilder.Append("- Запущено процедуру оновлення сертифікатів!");
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Завантаження сертифікатів із сервера...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (File.Exists(text))
		{
			FileSystem.DeleteFile(text);
		}
		if (File.Exists(text3))
		{
			FileSystem.DeleteFile(text3);
		}
		try
		{
			MyProject.Computer.Network.DownloadFile(address, text);
			MyProject.Computer.Network.DownloadFile(address2, text3);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Помилка: " + ex2.Message);
			TestResult.Text = stringBuilder.ToString();
			ProjectData.ClearProjectError();
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- ОК!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Перевірка файлів...");
		if (!File.Exists(text))
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Помилка");
			TestResult.Text = stringBuilder.ToString();
			return;
		}
		if (!File.Exists(text3))
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Помилка");
			TestResult.Text = stringBuilder.ToString();
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- ОК!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (File.Exists(text2))
		{
			FileSystem.DeleteFile(text2);
		}
		if (File.Exists(text4))
		{
			FileSystem.DeleteFile(text4);
		}
		FileSystem.RenameFile(text, newName);
		FileSystem.RenameFile(text3, newName2);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Сертифікати оновлено успішно!");
		TestResult.Text = stringBuilder.ToString();
	}
}
