using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
public class FormAccountant : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("ДодатиПідприємствоToolStripMenuItem")]
	private ToolStripMenuItem _ДодатиПідприємствоToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("CBorg")]
	private ComboBox _CBorg;

	[CompilerGenerated]
	[AccessedThroughProperty("LBfn")]
	private CheckedListBox _LBfn;

	[CompilerGenerated]
	[AccessedThroughProperty("UpdateB")]
	private Button _UpdateB;

	[CompilerGenerated]
	[AccessedThroughProperty("ChecksB")]
	private Button _ChecksB;

	[CompilerGenerated]
	[AccessedThroughProperty("ReportsB")]
	private Button _ReportsB;

	[CompilerGenerated]
	[AccessedThroughProperty("SearchB")]
	private Button _SearchB;

	[CompilerGenerated]
	[AccessedThroughProperty("SeaT")]
	private TextBox _SeaT;

	[CompilerGenerated]
	[AccessedThroughProperty("NoSB")]
	private Button _NoSB;

	[CompilerGenerated]
	[AccessedThroughProperty("ОновитиДаніПідприємстваToolStripMenuItem")]
	private ToolStripMenuItem _ОновитиДаніПідприємстваToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЕкспортЧеківЗаПеріодToolStripMenuItem")]
	private ToolStripMenuItem _ЕкспортЧеківЗаПеріодToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ДоступніЛіцензіїToolStripMenuItem")]
	private ToolStripMenuItem _ДоступніЛіцензіїToolStripMenuItem;

	private AccountantОffice AO;

	private bool fullD;

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

	internal virtual ToolStripMenuItem ДодатиПідприємствоToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДодатиПідприємствоToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ДодатиПідприємствоToolStripMenuItem_Click;
			ToolStripMenuItem додатиПідприємствоToolStripMenuItem = _ДодатиПідприємствоToolStripMenuItem;
			if (додатиПідприємствоToolStripMenuItem != null)
			{
				додатиПідприємствоToolStripMenuItem.Click -= value2;
			}
			_ДодатиПідприємствоToolStripMenuItem = value;
			додатиПідприємствоToolStripMenuItem = _ДодатиПідприємствоToolStripMenuItem;
			if (додатиПідприємствоToolStripMenuItem != null)
			{
				додатиПідприємствоToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual ComboBox CBorg
	{
		[CompilerGenerated]
		get
		{
			return _CBorg;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CBorg_SelectedIndexChanged;
			ComboBox cBorg = _CBorg;
			if (cBorg != null)
			{
				cBorg.SelectedIndexChanged -= value2;
			}
			_CBorg = value;
			cBorg = _CBorg;
			if (cBorg != null)
			{
				cBorg.SelectedIndexChanged += value2;
			}
		}
	}

	internal virtual CheckedListBox LBfn
	{
		[CompilerGenerated]
		get
		{
			return _LBfn;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = LBfn_SelectedIndexChanged;
			CheckedListBox lBfn = _LBfn;
			if (lBfn != null)
			{
				lBfn.SelectedIndexChanged -= value2;
			}
			_LBfn = value;
			lBfn = _LBfn;
			if (lBfn != null)
			{
				lBfn.SelectedIndexChanged += value2;
			}
		}
	}

	internal virtual Button UpdateB
	{
		[CompilerGenerated]
		get
		{
			return _UpdateB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = UpdateB_Click;
			Button updateB = _UpdateB;
			if (updateB != null)
			{
				updateB.Click -= value2;
			}
			_UpdateB = value;
			updateB = _UpdateB;
			if (updateB != null)
			{
				updateB.Click += value2;
			}
		}
	}

	internal virtual Button ChecksB
	{
		[CompilerGenerated]
		get
		{
			return _ChecksB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ChecksB_Click;
			Button checksB = _ChecksB;
			if (checksB != null)
			{
				checksB.Click -= value2;
			}
			_ChecksB = value;
			checksB = _ChecksB;
			if (checksB != null)
			{
				checksB.Click += value2;
			}
		}
	}

	internal virtual Button ReportsB
	{
		[CompilerGenerated]
		get
		{
			return _ReportsB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ReportsB_Click;
			Button reportsB = _ReportsB;
			if (reportsB != null)
			{
				reportsB.Click -= value2;
			}
			_ReportsB = value;
			reportsB = _ReportsB;
			if (reportsB != null)
			{
				reportsB.Click += value2;
			}
		}
	}

	internal virtual Button SearchB
	{
		[CompilerGenerated]
		get
		{
			return _SearchB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = SearchB_Click;
			Button searchB = _SearchB;
			if (searchB != null)
			{
				searchB.Click -= value2;
			}
			_SearchB = value;
			searchB = _SearchB;
			if (searchB != null)
			{
				searchB.Click += value2;
			}
		}
	}

	internal virtual TextBox SeaT
	{
		[CompilerGenerated]
		get
		{
			return _SeaT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			MouseEventHandler value2 = SeaT_MouseDoubleClick;
			TextBox seaT = _SeaT;
			if (seaT != null)
			{
				seaT.MouseDoubleClick -= value2;
			}
			_SeaT = value;
			seaT = _SeaT;
			if (seaT != null)
			{
				seaT.MouseDoubleClick += value2;
			}
		}
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("IndiT")]
	internal virtual TextBox IndiT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DateUpdadeT")]
	internal virtual TextBox DateUpdadeT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button NoSB
	{
		[CompilerGenerated]
		get
		{
			return _NoSB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = NoSB_Click;
			Button noSB = _NoSB;
			if (noSB != null)
			{
				noSB.Click -= value2;
			}
			_NoSB = value;
			noSB = _NoSB;
			if (noSB != null)
			{
				noSB.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem ОновитиДаніПідприємстваToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОновитиДаніПідприємстваToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ОновитиДаніПідприємстваToolStripMenuItem_Click;
			ToolStripMenuItem оновитиДаніПідприємстваToolStripMenuItem = _ОновитиДаніПідприємстваToolStripMenuItem;
			if (оновитиДаніПідприємстваToolStripMenuItem != null)
			{
				оновитиДаніПідприємстваToolStripMenuItem.Click -= value2;
			}
			_ОновитиДаніПідприємстваToolStripMenuItem = value;
			оновитиДаніПідприємстваToolStripMenuItem = _ОновитиДаніПідприємстваToolStripMenuItem;
			if (оновитиДаніПідприємстваToolStripMenuItem != null)
			{
				оновитиДаніПідприємстваToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ЕкспортЧеківЗаПеріодToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЕкспортЧеківЗаПеріодToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ЕкспортЧеківЗаПеріодToolStripMenuItem_Click;
			ToolStripMenuItem експортЧеківЗаПеріодToolStripMenuItem = _ЕкспортЧеківЗаПеріодToolStripMenuItem;
			if (експортЧеківЗаПеріодToolStripMenuItem != null)
			{
				експортЧеківЗаПеріодToolStripMenuItem.Click -= value2;
			}
			_ЕкспортЧеківЗаПеріодToolStripMenuItem = value;
			експортЧеківЗаПеріодToolStripMenuItem = _ЕкспортЧеківЗаПеріодToolStripMenuItem;
			if (експортЧеківЗаПеріодToolStripMenuItem != null)
			{
				експортЧеківЗаПеріодToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem2")]
	internal virtual ToolStripSeparator ToolStripMenuItem2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ДоступніЛіцензіїToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДоступніЛіцензіїToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ДоступніЛіцензіїToolStripMenuItem_Click;
			ToolStripMenuItem доступніЛіцензіїToolStripMenuItem = _ДоступніЛіцензіїToolStripMenuItem;
			if (доступніЛіцензіїToolStripMenuItem != null)
			{
				доступніЛіцензіїToolStripMenuItem.Click -= value2;
			}
			_ДоступніЛіцензіїToolStripMenuItem = value;
			доступніЛіцензіїToolStripMenuItem = _ДоступніЛіцензіїToolStripMenuItem;
			if (доступніЛіцензіїToolStripMenuItem != null)
			{
				доступніЛіцензіїToolStripMenuItem.Click += value2;
			}
		}
	}

	public FormAccountant()
	{
		base.Load += FormAccountant_Load;
		AO = new AccountantОffice();
		fullD = false;
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
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormAccountant));
		this.MenuStrip1 = new System.Windows.Forms.MenuStrip();
		this.МенюToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ДодатиПідприємствоToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ОновитиДаніПідприємстваToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem1 = new System.Windows.Forms.ToolStripSeparator();
		this.ЕкспортЧеківЗаПеріодToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.CBorg = new System.Windows.Forms.ComboBox();
		this.LBfn = new System.Windows.Forms.CheckedListBox();
		this.UpdateB = new System.Windows.Forms.Button();
		this.ChecksB = new System.Windows.Forms.Button();
		this.ReportsB = new System.Windows.Forms.Button();
		this.SearchB = new System.Windows.Forms.Button();
		this.SeaT = new System.Windows.Forms.TextBox();
		this.Label2 = new System.Windows.Forms.Label();
		this.IndiT = new System.Windows.Forms.TextBox();
		this.DateUpdadeT = new System.Windows.Forms.TextBox();
		this.NoSB = new System.Windows.Forms.Button();
		this.ToolStripMenuItem2 = new System.Windows.Forms.ToolStripSeparator();
		this.ДоступніЛіцензіїToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.MenuStrip1.SuspendLayout();
		base.SuspendLayout();
		this.MenuStrip1.ImageScalingSize = new System.Drawing.Size(20, 20);
		this.MenuStrip1.Items.AddRange(new System.Windows.Forms.ToolStripItem[1] { this.МенюToolStripMenuItem });
		this.MenuStrip1.Location = new System.Drawing.Point(0, 0);
		this.MenuStrip1.Name = "MenuStrip1";
		this.MenuStrip1.Size = new System.Drawing.Size(1129, 28);
		this.MenuStrip1.TabIndex = 0;
		this.MenuStrip1.Text = "MenuStrip1";
		this.МенюToolStripMenuItem.DropDownItems.AddRange(new System.Windows.Forms.ToolStripItem[6] { this.ДодатиПідприємствоToolStripMenuItem, this.ОновитиДаніПідприємстваToolStripMenuItem, this.ToolStripMenuItem2, this.ДоступніЛіцензіїToolStripMenuItem, this.ToolStripMenuItem1, this.ЕкспортЧеківЗаПеріодToolStripMenuItem });
		this.МенюToolStripMenuItem.Name = "МенюToolStripMenuItem";
		this.МенюToolStripMenuItem.Size = new System.Drawing.Size(65, 24);
		this.МенюToolStripMenuItem.Text = "Меню";
		this.ДодатиПідприємствоToolStripMenuItem.Name = "ДодатиПідприємствоToolStripMenuItem";
		this.ДодатиПідприємствоToolStripMenuItem.Size = new System.Drawing.Size(294, 26);
		this.ДодатиПідприємствоToolStripMenuItem.Text = "Додати підприємство...";
		this.ОновитиДаніПідприємстваToolStripMenuItem.Name = "ОновитиДаніПідприємстваToolStripMenuItem";
		this.ОновитиДаніПідприємстваToolStripMenuItem.Size = new System.Drawing.Size(294, 26);
		this.ОновитиДаніПідприємстваToolStripMenuItem.Text = "Оновити дані підприємства...";
		this.ToolStripMenuItem1.Name = "ToolStripMenuItem1";
		this.ToolStripMenuItem1.Size = new System.Drawing.Size(291, 6);
		this.ЕкспортЧеківЗаПеріодToolStripMenuItem.Name = "ЕкспортЧеківЗаПеріодToolStripMenuItem";
		this.ЕкспортЧеківЗаПеріодToolStripMenuItem.Size = new System.Drawing.Size(294, 26);
		this.ЕкспортЧеківЗаПеріодToolStripMenuItem.Text = "Експорт чеків за період...";
		this.CBorg.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
		this.CBorg.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CBorg.FormattingEnabled = true;
		this.CBorg.Location = new System.Drawing.Point(129, 41);
		this.CBorg.Name = "CBorg";
		this.CBorg.Size = new System.Drawing.Size(933, 33);
		this.CBorg.TabIndex = 1;
		this.LBfn.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left | System.Windows.Forms.AnchorStyles.Right;
		this.LBfn.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.LBfn.FormattingEnabled = true;
		this.LBfn.HorizontalScrollbar = true;
		this.LBfn.Location = new System.Drawing.Point(12, 140);
		this.LBfn.Name = "LBfn";
		this.LBfn.Size = new System.Drawing.Size(1105, 444);
		this.LBfn.TabIndex = 2;
		this.UpdateB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.UpdateB.Location = new System.Drawing.Point(12, 83);
		this.UpdateB.Name = "UpdateB";
		this.UpdateB.Size = new System.Drawing.Size(137, 47);
		this.UpdateB.TabIndex = 3;
		this.UpdateB.Text = "Оновити";
		this.UpdateB.UseVisualStyleBackColor = true;
		this.ChecksB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ChecksB.Location = new System.Drawing.Point(335, 83);
		this.ChecksB.Name = "ChecksB";
		this.ChecksB.Size = new System.Drawing.Size(173, 47);
		this.ChecksB.TabIndex = 4;
		this.ChecksB.Text = "Зміни та чеки";
		this.ChecksB.UseVisualStyleBackColor = true;
		this.ReportsB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.ReportsB.Location = new System.Drawing.Point(514, 83);
		this.ReportsB.Name = "ReportsB";
		this.ReportsB.Size = new System.Drawing.Size(173, 47);
		this.ReportsB.TabIndex = 5;
		this.ReportsB.Text = "Звіти за період";
		this.ReportsB.UseVisualStyleBackColor = true;
		this.SearchB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SearchB.Location = new System.Drawing.Point(924, 83);
		this.SearchB.Name = "SearchB";
		this.SearchB.Size = new System.Drawing.Size(138, 47);
		this.SearchB.TabIndex = 6;
		this.SearchB.Text = "Пошук";
		this.SearchB.UseVisualStyleBackColor = true;
		this.SeaT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SeaT.Location = new System.Drawing.Point(706, 92);
		this.SeaT.Name = "SeaT";
		this.SeaT.Size = new System.Drawing.Size(205, 30);
		this.SeaT.TabIndex = 7;
		this.SeaT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(12, 48);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(107, 20);
		this.Label2.TabIndex = 9;
		this.Label2.Text = "Організація";
		this.IndiT.Enabled = false;
		this.IndiT.Font = new System.Drawing.Font("Microsoft Sans Serif", 13.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.IndiT.Location = new System.Drawing.Point(1068, 41);
		this.IndiT.Name = "IndiT";
		this.IndiT.ReadOnly = true;
		this.IndiT.Size = new System.Drawing.Size(49, 34);
		this.IndiT.TabIndex = 10;
		this.IndiT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.DateUpdadeT.BackColor = System.Drawing.SystemColors.Window;
		this.DateUpdadeT.Enabled = false;
		this.DateUpdadeT.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.DateUpdadeT.Location = new System.Drawing.Point(155, 83);
		this.DateUpdadeT.Multiline = true;
		this.DateUpdadeT.Name = "DateUpdadeT";
		this.DateUpdadeT.ReadOnly = true;
		this.DateUpdadeT.Size = new System.Drawing.Size(174, 47);
		this.DateUpdadeT.TabIndex = 11;
		this.DateUpdadeT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.NoSB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.NoSB.Location = new System.Drawing.Point(1068, 83);
		this.NoSB.Name = "NoSB";
		this.NoSB.Size = new System.Drawing.Size(49, 47);
		this.NoSB.TabIndex = 12;
		this.NoSB.Text = "X";
		this.NoSB.UseVisualStyleBackColor = true;
		this.ToolStripMenuItem2.Name = "ToolStripMenuItem2";
		this.ToolStripMenuItem2.Size = new System.Drawing.Size(291, 6);
		this.ДоступніЛіцензіїToolStripMenuItem.Name = "ДоступніЛіцензіїToolStripMenuItem";
		this.ДоступніЛіцензіїToolStripMenuItem.Size = new System.Drawing.Size(294, 26);
		this.ДоступніЛіцензіїToolStripMenuItem.Text = "Доступні ліцензії...";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1129, 595);
		base.Controls.Add(this.NoSB);
		base.Controls.Add(this.DateUpdadeT);
		base.Controls.Add(this.IndiT);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.SeaT);
		base.Controls.Add(this.SearchB);
		base.Controls.Add(this.ReportsB);
		base.Controls.Add(this.ChecksB);
		base.Controls.Add(this.UpdateB);
		base.Controls.Add(this.LBfn);
		base.Controls.Add(this.CBorg);
		base.Controls.Add(this.MenuStrip1);
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MainMenuStrip = this.MenuStrip1;
		base.MaximizeBox = false;
		base.Name = "FormAccountant";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Перегляд підприємств";
		this.MenuStrip1.ResumeLayout(false);
		this.MenuStrip1.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormAccountant_Load(object sender, EventArgs e)
	{
		Text += "           ( WebCheck0.dll  v.6.0.8.1368 )";
		UpdateB.Enabled = false;
		ChecksB.Enabled = false;
		ReportsB.Enabled = false;
		SearchB.Enabled = false;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		NoSB.Enabled = SearchB.Enabled;
		OrgS();
	}

	private void ДодатиПідприємствоToolStripMenuItem_Click(object sender, EventArgs e)
	{
		LBfn.Items.Clear();
		All.TinBux = "";
		new FormAddTin().ShowDialog();
		OrgS();
		if (All.TinBux.Length > 1)
		{
			CBorg.Text = All.TinBux;
		}
	}

	private void CBorg_SelectedIndexChanged(object sender, EventArgs e)
	{
		fullD = false;
		UpdateB.Enabled = false;
		ChecksB.Enabled = false;
		ReportsB.Enabled = false;
		SearchB.Enabled = true;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		NoSB.Enabled = SearchB.Enabled;
		SeaT.Text = "";
		DateUpdadeT.Text = "";
		string text = AO.TextToNumber(CBorg.Text);
		FnS(text);
		string fillePath = All.MyDoc() + "\\WebCheck\\Archive\\All\\" + AO.NameFileTIN(text) + ".p7s";
		string text2 = AO.SendFile(fillePath);
		if (Operators.CompareString(text2, "", TextCompare: false) != 0 && AO.Dereban(text2))
		{
			fullD = true;
		}
		if (fullD)
		{
			IndiT.BackColor = Color.LightGreen;
		}
		else
		{
			IndiT.BackColor = Color.LightPink;
		}
	}

	private void OrgS()
	{
		ComboBox.ObjectCollection items = CBorg.Items;
		items.Clear();
		int num = All.ArS.IndexMaxFn();
		for (int i = 1; i <= num; i = checked(i + 1))
		{
			string text = All.ArS.NameFn(i);
			items.Add(text + "   -   " + All.ArS.StringGetFn(text, "OrgName"));
		}
		items = null;
	}

	private void FnS(string eT)
	{
		CheckedListBox.ObjectCollection items = LBfn.Items;
		items.Clear();
		checked
		{
			int num = AO.CounterKeysTin(eT) - 1;
			for (int i = 0; i <= num; i++)
			{
				string text = All.ArS.StringGetFn(eT, AO.NameKeyINI("Fn", Conversions.ToInteger(i.ToString()))) + "    ";
				text += NameL(All.ArS.StringGetFn(eT, AO.NameKeyINI("Na", Conversions.ToInteger(i.ToString()))));
				text = text + "   " + All.ArS.StringGetFn(eT, AO.NameKeyINI("Ad", Conversions.ToInteger(i.ToString())));
				items.Add(text);
			}
			items = null;
		}
	}

	private void FnSeaS(string eT)
	{
		CheckedListBox.ObjectCollection items = LBfn.Items;
		items.Clear();
		checked
		{
			int num = AO.CounterKeysTin(eT) - 1;
			for (int i = 0; i <= num; i++)
			{
				string text = All.ArS.StringGetFn(eT, AO.NameKeyINI("Fn", Conversions.ToInteger(i.ToString()))) + "    ";
				text += NameL(All.ArS.StringGetFn(eT, AO.NameKeyINI("Na", Conversions.ToInteger(i.ToString()))));
				text = text + "   " + All.ArS.StringGetFn(eT, AO.NameKeyINI("Ad", Conversions.ToInteger(i.ToString())));
				if (text.ToLower().IndexOf(SeaT.Text.ToLower()) > -1)
				{
					items.Add(text);
				}
			}
			items = null;
		}
	}

	private string NameL(string eN, int eL = 25)
	{
		if (eN.Length > eL)
		{
			return eN.Substring(0, eL);
		}
		if (eN.Length < eL)
		{
			eN += Strings.Space(checked((int)Math.Round((double)(eL - eN.Length) * 1.9)));
		}
		return eN;
	}

	private void LBfn_SelectedIndexChanged(object sender, EventArgs e)
	{
		checked
		{
			int num = LBfn.Items.Count - 1;
			for (int i = 0; i <= num; i++)
			{
				LBfn.SetItemChecked(i, value: false);
				try
				{
					LBfn.SetItemChecked(LBfn.SelectedIndex, value: true);
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					UpdateB.Enabled = false;
					ChecksB.Enabled = false;
					ReportsB.Enabled = false;
					ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
					ProjectData.ClearProjectError();
					return;
				}
			}
			if (Operators.ConditionalCompareObjectNotEqual(LBfn.SelectedItem, null, TextCompare: false))
			{
				CopyLocalBase(LBfn.SelectedItem.ToString());
			}
			string text = AO.TextToNumber(LBfn.SelectedItem.ToString());
			if (File.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + text + "\\" + text + ".db"))
			{
				ChecksB.Enabled = true;
				ReportsB.Enabled = true;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = true;
			}
			else
			{
				ChecksB.Enabled = false;
				ReportsB.Enabled = false;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
			}
			UpdateB.Enabled = true;
			SearchB.Enabled = true;
			NoSB.Enabled = SearchB.Enabled;
		}
	}

	private string ShowReortDate(string ef)
	{
		ef = AO.TextToNumber(ef);
		object obj = All.MyDoc() + "\\WebCheck\\Archive\\" + ef + "\\" + ef + ".db";
		if (File.Exists(Conversions.ToString(obj)))
		{
			All.TestRegion();
			All.A.FN = ef;
			All.A.Connection = Conversions.ToString(Operators.AddObject(Operators.AddObject("Data Source=", obj), "; Version=3"));
			All.PayTax.StartLoad();
			FormDate formDate = new FormDate();
			formDate.ShowDialog();
			formDate.Dispose();
			All.A.Connection = "";
			All.A.Status = false;
			return "";
		}
		All.A.FN = "";
		All.A.Status = false;
		return "ошибка";
	}

	private string ShowReportsFN(string eF)
	{
		eF = AO.TextToNumber(eF);
		object obj = All.MyDoc() + "\\WebCheck\\Archive\\" + eF + "\\" + eF + ".db";
		if (File.Exists(Conversions.ToString(obj)))
		{
			All.TestRegion();
			All.A.FN = eF;
			All.A.Connection = Conversions.ToString(Operators.AddObject(Operators.AddObject("Data Source=", obj), "; Version=3"));
			All.PayTax.StartLoad();
			All.A.Status = true;
			FormReports formReports = new FormReports();
			formReports.ShowDialog();
			formReports.Dispose();
			All.A.Connection = "";
			All.A.Status = false;
			return "";
		}
		All.A.FN = "";
		All.A.Status = false;
		return "ошибка";
	}

	private bool CopyLocalBase(string eF)
	{
		eF = AO.TextToNumber(eF);
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + eF + ".db";
		string destinationFileName = All.MyDoc() + "\\WebCheck\\Archive\\" + eF + "\\" + eF + ".db";
		if (File.Exists(text))
		{
			MyProject.Computer.FileSystem.CopyFile(text, destinationFileName, overwrite: true);
			UpdateTime(AO.TextToNumber(CBorg.Text), eF);
			DateUpdadeT.Text = GetUpdateTime(AO.TextToNumber(CBorg.Text), eF);
			return true;
		}
		DateUpdadeT.Text = GetUpdateTime(AO.TextToNumber(CBorg.Text), eF);
		return false;
	}

	private void ChecksB_Click(object sender, EventArgs e)
	{
		ShowReportsFN(LBfn.SelectedItem.ToString());
	}

	private void ReportsB_Click(object sender, EventArgs e)
	{
		ShowReortDate(LBfn.SelectedItem.ToString());
	}

	private void UpdateB_Click(object sender, EventArgs e)
	{
		if (!CopyLocalBase(LBfn.SelectedItem.ToString()))
		{
			TypLoadDB e2 = default(TypLoadDB);
			e2.Tin = AO.TextToNumber(CBorg.Text);
			e2.FN = AO.TextToNumber(LBfn.Text);
			e2.Update = GetUpdateTime(e2.Tin, e2.FN);
			e2.Online = fullD;
			FormLoadDB formLoadDB = new FormLoadDB(e2);
			formLoadDB.ShowDialog();
			formLoadDB.Dispose();
			DateUpdadeT.Text = GetUpdateTime(e2.Tin, e2.FN);
			string text = AO.TextToNumber(LBfn.SelectedItem.ToString());
			if (File.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + text + "\\" + text + ".db"))
			{
				ChecksB.Enabled = true;
				ReportsB.Enabled = true;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = true;
			}
			else
			{
				ChecksB.Enabled = false;
				ReportsB.Enabled = false;
				ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
			}
			UpdateB.Enabled = true;
			SearchB.Enabled = true;
			NoSB.Enabled = SearchB.Enabled;
		}
	}

	private void UpdateTime(string eTin, string eFn)
	{
		int num = AO.IndexKeysTin(eTin, eFn);
		All.ArS.StringWriteFN(eTin, AO.NameKeyINI("UP", Conversions.ToInteger(num.ToString())), "L " + DateTime.Now.ToString());
	}

	private string GetUpdateTime(string eTin, string eFn)
	{
		int num = AO.IndexKeysTin(eTin, eFn);
		string text = All.ArS.StringGetFn(eTin, AO.NameKeyINI("UP", Conversions.ToInteger(num.ToString())));
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			text = "no update";
		}
		return text;
	}

	private void SearchB_Click(object sender, EventArgs e)
	{
		SearchB.Enabled = false;
		string eT = AO.TextToNumber(CBorg.Text);
		if (Operators.CompareString(SeaT.Text.Trim(), "", TextCompare: false) == 0)
		{
			FnS(eT);
			SearchB.Enabled = true;
			return;
		}
		FnSeaS(eT);
		UpdateB.Enabled = false;
		ChecksB.Enabled = false;
		ReportsB.Enabled = false;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		SearchB.Enabled = true;
	}

	private void SeaT_MouseDoubleClick(object sender, MouseEventArgs e)
	{
		SeaT.Text = "";
	}

	private void NoSB_Click(object sender, EventArgs e)
	{
		NoSB.Enabled = false;
		SeaT.Text = "";
		string eT = AO.TextToNumber(CBorg.Text);
		FnS(eT);
		UpdateB.Enabled = false;
		ChecksB.Enabled = false;
		ReportsB.Enabled = false;
		ЕкспортЧеківЗаПеріодToolStripMenuItem.Enabled = false;
		NoSB.Enabled = true;
	}

	private void ОновитиДаніПідприємстваToolStripMenuItem_Click(object sender, EventArgs e)
	{
		LBfn.Items.Clear();
		All.TinBux = "";
		new FormAddTin(e: false).ShowDialog();
		OrgS();
		if (All.TinBux.Length > 1)
		{
			CBorg.Text = All.TinBux;
		}
	}

	private void ЕкспортЧеківЗаПеріодToolStripMenuItem_Click(object sender, EventArgs e)
	{
		string text = AO.TextToNumber(LBfn.SelectedItem.ToString());
		object obj = All.MyDoc() + "\\WebCheck\\Archive\\" + text + "\\" + text + ".db";
		if (File.Exists(Conversions.ToString(obj)))
		{
			All.TestRegion();
			All.A.FN = text;
			All.A.Connection = Conversions.ToString(Operators.AddObject(Operators.AddObject("Data Source=", obj), "; Version=3"));
			All.PayTax.StartLoad();
			All.A.Status = true;
			ExportingToText exportingToText = new ExportingToText(text);
			exportingToText.ShowDialog();
			exportingToText.Dispose();
			All.A.Connection = "";
			All.A.Status = false;
		}
		else
		{
			All.A.FN = "";
			All.A.Status = false;
			All.A.Connection = "";
		}
	}

	private void ДоступніЛіцензіїToolStripMenuItem_Click(object sender, EventArgs e)
	{
		new FormLicenses().ShowDialog();
	}
}
